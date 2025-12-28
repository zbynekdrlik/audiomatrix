//! VBAN stream manager for cross-node audio routing.
//!
//! This module manages VBAN sender and receiver instances for cross-node
//! audio routing. It integrates with the subscription manager to create
//! and tear down VBAN streams automatically.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use ram_core::ring_buffer_pool::RingBufferPool;
use ram_core::routing_table::RoutingTable;
use ram_core::subscription::SubscriptionId;
use ram_core::subscription_manager::SubscriptionManager;
use ram_vban::{VbanReceiver, VbanSender, VbanSenderConfig};

/// Configuration for the VBAN manager.
#[derive(Debug, Clone)]
pub struct VbanManagerConfig {
    /// Local VBAN port for receiving.
    pub local_port: u16,
    /// Default sample rate for VBAN streams.
    pub sample_rate: u32,
}

impl Default for VbanManagerConfig {
    fn default() -> Self {
        Self {
            local_port: 6980,
            sample_rate: 48000,
        }
    }
}

/// Active VBAN sender state.
struct VbanSenderState {
    /// The sender instance.
    sender: Arc<VbanSender>,
    /// Task handle for the send loop.
    task: JoinHandle<()>,
    /// Associated subscription ID.
    subscription_id: SubscriptionId,
    /// Source buffer indices to read from.
    source_buffers: Vec<usize>,
}

/// Active VBAN receiver state.
struct VbanReceiverState {
    /// Task handle for the receive loop.
    task: JoinHandle<()>,
    /// Shutdown channel.
    shutdown_tx: mpsc::Sender<()>,
}

/// Manages VBAN senders and receivers for cross-node routing.
pub struct VbanManager {
    config: VbanManagerConfig,
    /// Active VBAN senders (subscription_id -> state).
    senders: RwLock<HashMap<SubscriptionId, VbanSenderState>>,
    /// Active VBAN receiver (one per node).
    receiver: RwLock<Option<VbanReceiverState>>,
    /// Buffer pool for ring buffers.
    buffer_pool: Arc<RingBufferPool>,
    /// Routing table for output routing.
    routing_table: Arc<RoutingTable>,
    /// Subscription manager.
    subscription_manager: Arc<SubscriptionManager>,
    /// Whether the manager is running.
    running: std::sync::atomic::AtomicBool,
}

impl VbanManager {
    /// Creates a new VBAN manager.
    pub fn new(
        config: VbanManagerConfig,
        buffer_pool: Arc<RingBufferPool>,
        routing_table: Arc<RoutingTable>,
        subscription_manager: Arc<SubscriptionManager>,
    ) -> Self {
        Self {
            config,
            senders: RwLock::new(HashMap::new()),
            receiver: RwLock::new(None),
            buffer_pool,
            routing_table,
            subscription_manager,
            running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Starts the VBAN receiver.
    pub async fn start_receiver(&self) -> Result<()> {
        if self.receiver.read().is_some() {
            return Ok(()); // Already running
        }

        let receiver = VbanReceiver::bind(self.config.local_port).await?;
        let buffer_pool = Arc::clone(&self.buffer_pool);
        let routing_table = Arc::clone(&self.routing_table);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        info!("Starting VBAN receiver on port {}", self.config.local_port);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = receiver.receive() => {
                        match result {
                            Ok((header, samples, addr)) => {
                                // Route received samples to appropriate buffers
                                Self::handle_received_vban(
                                    &header.stream_name,
                                    &samples,
                                    addr,
                                    &buffer_pool,
                                    &routing_table,
                                );
                            }
                            Err(e) => {
                                warn!("VBAN receive error: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("VBAN receiver shutting down");
                        break;
                    }
                }
            }
        });

        *self.receiver.write() = Some(VbanReceiverState { task, shutdown_tx });
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);

        info!("VBAN receiver started");
        Ok(())
    }

    /// Handles received VBAN samples.
    fn handle_received_vban(
        stream_name: &str,
        samples: &[f32],
        _addr: SocketAddr,
        buffer_pool: &RingBufferPool,
        _routing_table: &RoutingTable,
    ) {
        debug!(
            "Received VBAN stream '{}': {} samples",
            stream_name,
            samples.len()
        );

        // TODO: Look up the subscription by stream name to find the destination buffer
        // For now, just log that we received audio
        // This will be connected to the routing table once subscriptions are fully integrated
        let _ = buffer_pool;
    }

    /// Starts a VBAN sender for an outgoing subscription.
    pub async fn start_sender(
        &self,
        subscription_id: SubscriptionId,
        stream_name: String,
        destination: SocketAddr,
        source_buffers: Vec<usize>,
        channels: u8,
    ) -> Result<()> {
        if self.senders.read().contains_key(&subscription_id) {
            return Err(anyhow!(
                "Sender already exists for subscription {subscription_id}"
            ));
        }

        let config = VbanSenderConfig {
            stream_name: stream_name.clone(),
            sample_rate: self.config.sample_rate,
            channels,
            destination,
        };

        let sender = Arc::new(VbanSender::new(config).await?);
        let sender_clone = Arc::clone(&sender);
        let buffer_pool = Arc::clone(&self.buffer_pool);
        let buffers = source_buffers.clone();

        info!(
            "Starting VBAN sender for subscription {subscription_id}: stream='{}' -> {}",
            stream_name, destination
        );

        // Spawn send loop
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_micros(5333)); // ~188 Hz (256 samples @ 48kHz)

            loop {
                interval.tick().await;

                // Read samples from source buffers
                let mut samples = Vec::with_capacity(256 * buffers.len());
                for &buffer_idx in &buffers {
                    let mut channel_samples = [0.0f32; 256];
                    if let Some(buffer) = buffer_pool.get(buffer_idx) {
                        let read = buffer.read(&mut channel_samples);
                        if read < 256 {
                            // Underrun - pad with silence
                            channel_samples[read..].fill(0.0);
                        }
                    }
                    samples.extend_from_slice(&channel_samples);
                }

                // Send interleaved samples
                if let Err(e) = sender_clone.send(&samples).await {
                    error!("VBAN send error: {e}");
                }
            }
        });

        self.senders.write().insert(
            subscription_id,
            VbanSenderState {
                sender,
                task,
                subscription_id,
                source_buffers,
            },
        );

        info!("VBAN sender started for subscription {subscription_id}");
        Ok(())
    }

    /// Stops a VBAN sender.
    pub fn stop_sender(&self, subscription_id: SubscriptionId) -> Result<()> {
        if let Some(state) = self.senders.write().remove(&subscription_id) {
            state.task.abort();
            info!("VBAN sender stopped for subscription {subscription_id}");
            Ok(())
        } else {
            Err(anyhow!(
                "No sender found for subscription {subscription_id}"
            ))
        }
    }

    /// Stops the VBAN receiver.
    pub async fn stop_receiver(&self) -> Result<()> {
        if let Some(state) = self.receiver.write().take() {
            let _ = state.shutdown_tx.send(()).await;
            state.task.abort();
            info!("VBAN receiver stopped");
        }
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Stops all senders and the receiver.
    pub async fn stop_all(&self) {
        // Stop all senders
        let sender_ids: Vec<_> = self.senders.read().keys().copied().collect();
        for id in sender_ids {
            let _ = self.stop_sender(id);
        }

        // Stop receiver
        let _ = self.stop_receiver().await;

        info!("All VBAN streams stopped");
    }

    /// Returns whether the manager is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns the number of active senders.
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.senders.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ram_core::routing_snapshot::RoutingSnapshot;

    #[test]
    fn vban_manager_config_default() {
        let config = VbanManagerConfig::default();
        assert_eq!(config.local_port, 6980);
        assert_eq!(config.sample_rate, 48000);
    }

    #[tokio::test]
    async fn vban_manager_creation() {
        let config = VbanManagerConfig::default();
        let buffer_pool = Arc::new(RingBufferPool::new(16, 2048));
        let routing_table = Arc::new(RoutingTable::new(RoutingSnapshot::new()));
        let sub_manager = Arc::new(SubscriptionManager::with_defaults("test".to_string()));

        let manager = VbanManager::new(config, buffer_pool, routing_table, sub_manager);

        assert!(!manager.is_running());
        assert_eq!(manager.sender_count(), 0);
    }
}
