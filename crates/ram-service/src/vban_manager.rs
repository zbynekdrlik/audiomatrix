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
    /// Mapping from VBAN stream name to buffer indices for received audio.
    /// This maps stream names to the buffer(s) where received audio should be written.
    stream_buffers: Arc<RwLock<HashMap<String, Vec<usize>>>>,
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
            stream_buffers: Arc::new(RwLock::new(HashMap::new())),
            running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Registers buffer indices for a VBAN stream.
    ///
    /// Call this when a subscription is confirmed to map the stream name
    /// to the buffer(s) where received audio should be written.
    pub fn register_stream_buffers(&self, stream_name: &str, buffer_indices: Vec<usize>) {
        info!(
            "Registering VBAN stream '{}' with buffer indices {:?}",
            stream_name, buffer_indices
        );
        self.stream_buffers
            .write()
            .insert(stream_name.to_string(), buffer_indices);
    }

    /// Unregisters buffer indices for a VBAN stream.
    pub fn unregister_stream(&self, stream_name: &str) {
        self.stream_buffers.write().remove(stream_name);
    }

    /// Starts the VBAN receiver.
    pub async fn start_receiver(&self) -> Result<()> {
        if self.receiver.read().is_some() {
            return Ok(()); // Already running
        }

        let receiver = VbanReceiver::bind(self.config.local_port).await?;
        let buffer_pool = Arc::clone(&self.buffer_pool);
        let stream_buffers = Arc::clone(&self.stream_buffers);
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
                                    &stream_buffers,
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
        stream_buffers: &Arc<RwLock<HashMap<String, Vec<usize>>>>,
    ) {
        // Look up buffer indices for this stream
        let buffer_indices = {
            let buffers = stream_buffers.read();
            buffers.get(stream_name).cloned()
        };

        let Some(buffer_indices) = buffer_indices else {
            // Stream not registered - this is normal for unsubscribed streams
            // Use warn for debugging - we should be registering streams
            warn!(
                "Received VBAN stream '{}' but it is not registered!",
                stream_name
            );
            return;
        };

        // Calculate samples per channel
        let channels = buffer_indices.len();
        if channels == 0 {
            return;
        }

        let samples_per_channel = samples.len() / channels;
        if samples_per_channel == 0 {
            return;
        }

        // De-interleave and write to ring buffers
        for (ch, &buffer_idx) in buffer_indices.iter().enumerate() {
            if let Some(ring_buffer) = buffer_pool.get(buffer_idx) {
                // Extract samples for this channel
                let channel_samples: Vec<f32> = (0..samples_per_channel)
                    .map(|i| samples[i * channels + ch])
                    .collect();
                ring_buffer.write(&channel_samples);
            }
        }

        // Log periodically (every 100th packet approximately)
        // Use a simple heuristic based on samples_per_channel
        if samples_per_channel == 256 {
            // This is a typical packet size, log occasionally
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 100 == 0 {
                info!(
                    "VBAN receive: {} packets received for stream '{}' -> {} buffers",
                    count + 1,
                    stream_name,
                    channels
                );
            }
        }
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
        let stream_name_for_log = stream_name.clone();
        let dest_for_log = destination;
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_micros(5333)); // ~188 Hz (256 samples @ 48kHz)
            let mut packet_count: u64 = 0;
            let mut last_log_time = std::time::Instant::now();

            debug!(
                "VBAN sender loop started for stream '{}' -> {}",
                stream_name_for_log, dest_for_log
            );

            loop {
                interval.tick().await;

                // Read samples from source buffers
                let mut samples = Vec::with_capacity(256 * buffers.len());
                let mut total_read = 0;
                for &buffer_idx in &buffers {
                    let mut channel_samples = [0.0f32; 256];
                    if let Some(buffer) = buffer_pool.get(buffer_idx) {
                        let read = buffer.read(&mut channel_samples);
                        total_read += read;
                        if read < 256 {
                            // Underrun - pad with silence
                            channel_samples[read..].fill(0.0);
                        }
                    } else {
                        warn!("VBAN sender: buffer {} not found in pool", buffer_idx);
                    }
                    samples.extend_from_slice(&channel_samples);
                }

                // Send interleaved samples
                if let Err(e) = sender_clone.send(&samples).await {
                    error!("VBAN send error: {e}");
                } else {
                    packet_count += 1;
                }

                // Log stats every 5 seconds
                if last_log_time.elapsed() >= std::time::Duration::from_secs(5) {
                    info!(
                        "VBAN sender '{}': {} packets sent, last read {} samples",
                        stream_name_for_log, packet_count, total_read
                    );
                    last_log_time = std::time::Instant::now();
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
        let routing_table = Arc::new(RoutingTable::new());
        let sub_manager = Arc::new(SubscriptionManager::with_defaults("test".to_string()));

        let manager = VbanManager::new(config, buffer_pool, routing_table, sub_manager);

        assert!(!manager.is_running());
        assert_eq!(manager.sender_count(), 0);
    }
}
