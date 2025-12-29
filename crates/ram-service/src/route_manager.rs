//! Route manager for audio routing operations.
//!
//! This module contains the `RouteManager` struct which implements the
//! `RouteController` trait for API integration. It manages the routing
//! table, buffer pool, and connection mappings.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use tracing::{debug, info};

use ram_core::latency::{LatencyCalculator, LatencyReport};
use ram_core::metering::MeterLevels;
use ram_core::ring_buffer_pool::RingBufferPool;
use ram_core::route_controller::{RouteController, RouteError, RouteResult};
use ram_core::routing_snapshot::DestinationSnapshot;
use ram_core::routing_table::RoutingTable;
use ram_core::stream_registry::StreamRegistry;
use ram_core::subscription::SubscriptionId;
use ram_core::subscription_manager::SubscriptionManager;
use ram_core::ConnectionId;

use crate::audio_processor::MeteringContexts;

use crate::vban_manager::VbanManager;

/// Callback for starting an input stream and getting buffer indices.
pub type StreamStarterFn =
    Arc<dyn Fn(&str, &[u16]) -> Result<Vec<usize>> + Send + Sync + 'static>;

/// Callback for starting an output stream.
pub type OutputStreamStarterFn = Arc<dyn Fn(&str) -> Result<()> + Send + Sync + 'static>;

/// Configuration for the route manager.
#[derive(Debug, Clone)]
pub struct RouteManagerConfig {
    /// Number of ring buffers to pre-allocate.
    pub buffer_pool_size: usize,
    /// Capacity of each ring buffer in samples.
    pub buffer_capacity: usize,
    /// Default sample rate.
    pub default_sample_rate: u32,
    /// Default buffer size per callback.
    pub default_buffer_size: u32,
    /// Local node name for subscription management.
    pub node_name: String,
}

impl Default for RouteManagerConfig {
    fn default() -> Self {
        Self {
            buffer_pool_size: 256,
            buffer_capacity: 2048,
            default_sample_rate: 48000,
            default_buffer_size: 256,
            node_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "local".to_string()),
        }
    }
}

/// Mapping from connection ID to buffer index in the pool.
#[derive(Debug, Default)]
pub(crate) struct ConnectionBufferMap {
    /// Connection ID -> buffer pool index
    pub connections: HashMap<ConnectionId, usize>,
}

impl ConnectionBufferMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, conn_id: ConnectionId, buffer_index: usize) {
        self.connections.insert(conn_id, buffer_index);
    }

    pub fn remove(&mut self, conn_id: &ConnectionId) -> Option<usize> {
        self.connections.remove(conn_id)
    }

    pub fn get(&self, conn_id: &ConnectionId) -> Option<usize> {
        self.connections.get(conn_id).copied()
    }

    pub fn contains(&self, conn_id: &ConnectionId) -> bool {
        self.connections.contains_key(conn_id)
    }
}

/// Route manager that implements RouteController.
///
/// This struct contains only the thread-safe parts of AudioProcessor
/// that can be shared across threads for API access.
pub struct RouteManager {
    /// Configuration.
    pub(crate) config: RouteManagerConfig,
    /// The RCU routing table.
    pub(crate) routing_table: Arc<RoutingTable>,
    /// Pre-allocated ring buffer pool.
    pub(crate) buffer_pool: Arc<RingBufferPool>,
    /// Active stream registry.
    pub(crate) stream_registry: Arc<StreamRegistry>,
    /// Subscription manager for cross-node routing.
    pub(crate) subscription_manager: Arc<SubscriptionManager>,
    /// Latency calculator.
    pub(crate) latency_calculator: LatencyCalculator,
    /// Connection ID to buffer index mapping.
    pub(crate) connection_buffers: RwLock<ConnectionBufferMap>,
    /// VBAN manager for cross-node audio (set after creation).
    vban_manager: RwLock<Option<Arc<VbanManager>>>,
    /// Callback for starting input streams (set after creation).
    stream_starter: RwLock<Option<StreamStarterFn>>,
    /// Callback for starting output streams (set after creation).
    output_stream_starter: RwLock<Option<OutputStreamStarterFn>>,
    /// Destination ID to snapshot index mapping.
    pub(crate) dest_indices: RwLock<HashMap<String, usize>>,
    /// Metering contexts for level data (set after creation).
    metering_contexts: RwLock<Option<Arc<MeteringContexts>>>,
}

impl RouteManager {
    /// Creates a new route manager with the given configuration.
    pub fn new(config: RouteManagerConfig) -> Self {
        let buffer_pool = Arc::new(RingBufferPool::new(
            config.buffer_pool_size,
            config.buffer_capacity,
        ));
        let routing_table = Arc::new(RoutingTable::new());
        let stream_registry = Arc::new(StreamRegistry::new());
        let subscription_manager =
            Arc::new(SubscriptionManager::with_defaults(config.node_name.clone()));
        let latency_calculator = LatencyCalculator::new(config.default_sample_rate);

        Self {
            config,
            routing_table,
            buffer_pool,
            stream_registry,
            subscription_manager,
            latency_calculator,
            connection_buffers: RwLock::new(ConnectionBufferMap::new()),
            vban_manager: RwLock::new(None),
            stream_starter: RwLock::new(None),
            output_stream_starter: RwLock::new(None),
            dest_indices: RwLock::new(HashMap::new()),
            metering_contexts: RwLock::new(None),
        }
    }

    /// Sets the VBAN manager for cross-node audio.
    ///
    /// This must be called after the VbanManager is created since it depends
    /// on resources from this RouteManager.
    pub fn set_vban_manager(&self, vban_manager: Arc<VbanManager>) {
        *self.vban_manager.write() = Some(vban_manager);
    }

    /// Sets the stream starter callback for starting input streams.
    ///
    /// This must be called after the AudioProcessor is fully initialized
    /// since it needs access to the audio backend.
    pub fn set_stream_starter(&self, starter: StreamStarterFn) {
        *self.stream_starter.write() = Some(starter);
    }

    /// Sets the output stream starter callback for starting output streams.
    ///
    /// This must be called after the AudioProcessor is fully initialized
    /// since it needs access to the audio backend.
    pub fn set_output_stream_starter(&self, starter: OutputStreamStarterFn) {
        *self.output_stream_starter.write() = Some(starter);
    }

    /// Sets the metering contexts for level data access.
    ///
    /// This must be called after the AudioProcessor is created since it owns
    /// the metering contexts.
    pub fn set_metering_contexts(&self, contexts: Arc<MeteringContexts>) {
        *self.metering_contexts.write() = Some(contexts);
    }

    /// Adds a route to the routing matrix.
    pub(crate) fn add_route_internal(&self, conn_id: ConnectionId) -> Result<usize> {
        // First, process any pending buffer frees from previous removals.
        // This ensures buffers are available for reuse.
        let current_gen = self.routing_table.generation();
        let freed = self.buffer_pool.process_pending_frees(current_gen);
        if freed > 0 {
            debug!("Processed {freed} pending buffer frees");
        }

        // Check if connection already exists
        if self.connection_buffers.read().contains(&conn_id) {
            return Err(anyhow!("Connection already exists: {conn_id}"));
        }

        // Allocate a buffer for this connection
        let buffer_idx = self
            .buffer_pool
            .allocate()
            .ok_or_else(|| anyhow!("Buffer pool exhausted"))?;

        // Store the mapping
        self.connection_buffers
            .write()
            .insert(conn_id.clone(), buffer_idx);

        // Update the routing table
        let dest_id = conn_id.destination_id();
        self.routing_table.update_with(|current| {
            let mut new = current.clone();
            if let Some(dest) = new.find_destination_mut(&dest_id) {
                dest.add_source(buffer_idx);
            } else {
                // Create destination if it doesn't exist
                let parts: Vec<&str> = dest_id.split(':').collect();
                let device_id = if parts.len() >= 2 {
                    parts[1]
                } else {
                    "unknown"
                };
                let channel: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

                let mut dest = DestinationSnapshot::new(
                    dest_id.clone(),
                    device_id.to_string(),
                    channel.saturating_sub(1),
                );
                dest.add_source(buffer_idx);
                new.destinations.push(dest);
            }
            new
        });

        info!("Added route {conn_id} with buffer index {buffer_idx}");
        Ok(buffer_idx)
    }

    /// Removes a route from the routing matrix.
    ///
    /// The buffer is not freed immediately - it's queued for deferred freeing
    /// to prevent race conditions with audio callbacks that may still hold
    /// cached routing snapshots referencing the buffer.
    pub(crate) fn remove_route_internal(&self, conn_id: &ConnectionId) -> Result<()> {
        // First, process any pending buffer frees from previous removals
        let current_gen = self.routing_table.generation();
        let freed = self.buffer_pool.process_pending_frees(current_gen);
        if freed > 0 {
            debug!("Processed {freed} pending buffer frees");
        }

        // Get and remove the buffer index from the connection map
        let buffer_idx = self
            .connection_buffers
            .write()
            .remove(conn_id)
            .ok_or_else(|| anyhow!("Connection not found: {conn_id}"))?;

        // Update the routing table (this increments the generation)
        let dest_id = conn_id.destination_id();
        self.routing_table.update_with(|current| {
            let mut new = current.clone();
            if let Some(dest) = new.find_destination_mut(&dest_id) {
                dest.remove_source(buffer_idx);
            }
            new
        });

        // Queue buffer for deferred freeing instead of immediate free.
        // The buffer will be actually freed after DEFER_FREE_GENERATIONS
        // routing updates, ensuring all audio callbacks have refreshed
        // their cached snapshots.
        let new_gen = self.routing_table.generation();
        self.buffer_pool.defer_free(buffer_idx, new_gen);

        info!(
            "Removed route {conn_id}, buffer index {buffer_idx} queued for deferred free (gen {})",
            new_gen
        );
        Ok(())
    }

    /// Sets the gain for a connection.
    pub(crate) fn set_connection_gain_internal(
        &self,
        conn_id: &ConnectionId,
        gain: f32,
    ) -> Result<()> {
        let buffer_idx = self
            .connection_buffers
            .read()
            .get(conn_id)
            .ok_or_else(|| anyhow!("Connection not found: {conn_id}"))?;

        let dest_id = conn_id.destination_id();
        let snapshot = self.routing_table.snapshot();

        if let Some(dest) = snapshot.find_destination(&dest_id) {
            if let Some(slot) = dest.find_source(buffer_idx) {
                slot.gain.set(gain);
                debug!("Set gain for {conn_id} to {gain}");
                return Ok(());
            }
        }

        Err(anyhow!("Source slot not found for connection {conn_id}"))
    }

    /// Sets the mute state for a connection.
    pub(crate) fn set_connection_muted_internal(
        &self,
        conn_id: &ConnectionId,
        muted: bool,
    ) -> Result<()> {
        let buffer_idx = self
            .connection_buffers
            .read()
            .get(conn_id)
            .ok_or_else(|| anyhow!("Connection not found: {conn_id}"))?;

        let dest_id = conn_id.destination_id();
        let snapshot = self.routing_table.snapshot();

        if let Some(dest) = snapshot.find_destination(&dest_id) {
            if let Some(slot) = dest.find_source(buffer_idx) {
                slot.muted
                    .store(muted, std::sync::atomic::Ordering::Relaxed);
                debug!("Set muted for {conn_id} to {muted}");
                return Ok(());
            }
        }

        Err(anyhow!("Source slot not found for connection {conn_id}"))
    }

    /// Calculates the expected latency for a route.
    pub(crate) fn calculate_route_latency_internal(&self, conn_id: &ConnectionId) -> LatencyReport {
        let input_buffer = self.config.default_buffer_size;
        let ring_buffer = self.config.buffer_capacity as u32;
        let output_buffer = self.config.default_buffer_size;

        // Check if this is a cross-node route
        let local = &self.config.node_name;
        let source_is_local = conn_id.source_node == "LOCAL" || conn_id.source_node == *local;
        let dest_is_local =
            conn_id.destination_node == "LOCAL" || conn_id.destination_node == *local;
        let is_cross_node = !source_is_local || !dest_is_local;

        if is_cross_node {
            // Network route: add jitter buffer and estimated network latency
            let jitter_buffer = 256u32; // 5.33ms @ 48kHz
            let network_rtt_ms = 2.0f32; // Estimated LAN RTT

            self.latency_calculator.network_latency(
                input_buffer,
                ring_buffer,
                jitter_buffer,
                network_rtt_ms,
                output_buffer,
            )
        } else {
            // Local route
            self.latency_calculator
                .local_latency(input_buffer, ring_buffer, output_buffer)
        }
    }
}

// ============================================================================
// RouteController Implementation
// ============================================================================

impl RouteController for RouteManager {
    fn add_route(&self, conn_id: ConnectionId) -> RouteResult<usize> {
        self.add_route_internal(conn_id).map_err(|e| {
            if e.to_string().contains("exhausted") {
                RouteError::NoBufferAvailable
            } else if e.to_string().contains("already exists") {
                RouteError::Invalid(e.to_string())
            } else {
                RouteError::Internal(e.to_string())
            }
        })
    }

    fn remove_route(&self, conn_id: &ConnectionId) -> RouteResult<()> {
        self.remove_route_internal(conn_id).map_err(|e| {
            if e.to_string().contains("not found") {
                RouteError::NotFound(conn_id.to_string())
            } else {
                RouteError::Internal(e.to_string())
            }
        })
    }

    fn set_route_gain(&self, conn_id: &ConnectionId, gain: f32) -> RouteResult<()> {
        self.set_connection_gain_internal(conn_id, gain)
            .map_err(|e| {
                if e.to_string().contains("not found") {
                    RouteError::NotFound(conn_id.to_string())
                } else {
                    RouteError::Internal(e.to_string())
                }
            })
    }

    fn set_route_muted(&self, conn_id: &ConnectionId, muted: bool) -> RouteResult<()> {
        self.set_connection_muted_internal(conn_id, muted)
            .map_err(|e| {
                if e.to_string().contains("not found") {
                    RouteError::NotFound(conn_id.to_string())
                } else {
                    RouteError::Internal(e.to_string())
                }
            })
    }

    fn has_route(&self, conn_id: &ConnectionId) -> bool {
        self.connection_buffers.read().contains(conn_id)
    }

    fn calculate_latency(&self, conn_id: &ConnectionId) -> LatencyReport {
        self.calculate_route_latency_internal(conn_id)
    }

    fn stream_registry(&self) -> &Arc<StreamRegistry> {
        &self.stream_registry
    }

    fn subscription_manager(&self) -> &Arc<SubscriptionManager> {
        &self.subscription_manager
    }

    fn node_name(&self) -> &str {
        &self.config.node_name
    }

    fn start_vban_sender(
        &self,
        subscription_id: SubscriptionId,
        stream_name: String,
        destination: SocketAddr,
        source_buffers: Vec<usize>,
        channels: u8,
    ) -> RouteResult<()> {
        let vban_manager_guard = self.vban_manager.read();
        let vban_manager = vban_manager_guard
            .as_ref()
            .ok_or_else(|| RouteError::Internal("VBAN manager not initialized".to_string()))?;

        // Clone what we need before dropping the lock
        let vban_manager = Arc::clone(vban_manager);
        drop(vban_manager_guard);

        // Use block_in_place to safely run async code from within an async context.
        // This is necessary because this method is called from an async axum handler,
        // and calling block_on directly would panic with "Cannot block from within a runtime".
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                vban_manager
                    .start_sender(subscription_id, stream_name, destination, source_buffers, channels)
                    .await
                    .map_err(|e| RouteError::Internal(e.to_string()))
            })
        })
    }

    fn stop_vban_sender(&self, subscription_id: SubscriptionId) -> RouteResult<()> {
        let vban_manager_guard = self.vban_manager.read();
        let vban_manager = vban_manager_guard
            .as_ref()
            .ok_or_else(|| RouteError::Internal("VBAN manager not initialized".to_string()))?;

        vban_manager
            .stop_sender(subscription_id)
            .map_err(|e| RouteError::Internal(e.to_string()))
    }

    fn ensure_input_stream(&self, device_id: &str, channels: &[u16]) -> RouteResult<Vec<usize>> {
        let starter_guard = self.stream_starter.read();
        let starter = starter_guard
            .as_ref()
            .ok_or_else(|| RouteError::Internal("Stream starter not initialized".to_string()))?;

        starter(device_id, channels).map_err(|e| RouteError::Internal(e.to_string()))
    }

    fn register_vban_stream_buffers(&self, stream_name: &str, buffer_indices: Vec<usize>) {
        let vban_manager_guard = self.vban_manager.read();
        if let Some(vban_manager) = vban_manager_guard.as_ref() {
            vban_manager.register_stream_buffers(stream_name, buffer_indices);
        } else {
            tracing::warn!(
                "Cannot register VBAN stream '{}': VBAN manager not initialized",
                stream_name
            );
        }
    }

    fn unregister_vban_stream(&self, stream_name: &str) {
        let vban_manager_guard = self.vban_manager.read();
        if let Some(vban_manager) = vban_manager_guard.as_ref() {
            vban_manager.unregister_stream(stream_name);
        }
    }

    fn allocate_receive_buffers(&self, dest_device: &str, channels: &[u16]) -> RouteResult<Vec<usize>> {
        let mut buffer_indices = Vec::with_capacity(channels.len());

        for ch in channels {
            match self.buffer_pool.allocate() {
                Some(idx) => {
                    buffer_indices.push(idx);
                    info!(
                        "Allocated receive buffer {} for {}:{} (VBAN incoming)",
                        idx, dest_device, ch
                    );
                }
                None => {
                    // Free any buffers we already allocated
                    for &idx in &buffer_indices {
                        self.buffer_pool.free(idx);
                    }
                    return Err(RouteError::NoBufferAvailable);
                }
            }
        }

        // Add buffers to routing table so output device can read from them
        for (i, &buffer_idx) in buffer_indices.iter().enumerate() {
            let channel = channels.get(i).copied().unwrap_or((i + 1) as u16);
            let dest_id = format!("LOCAL:{}:{}", dest_device, channel);

            self.routing_table.update_with(|current| {
                let mut new = current.clone();
                if let Some(dest) = new.find_destination_mut(&dest_id) {
                    dest.add_source(buffer_idx);
                    info!(
                        "Added VBAN receive buffer {} to existing destination {}",
                        buffer_idx, dest_id
                    );
                } else {
                    // Create destination if it doesn't exist
                    let channel_idx = (channel as usize).saturating_sub(1);
                    let mut dest = DestinationSnapshot::new(
                        dest_id.clone(),
                        dest_device.to_string(),
                        channel_idx,
                    );
                    dest.add_source(buffer_idx);
                    new.destinations.push(dest);
                    info!(
                        "Created destination {} with VBAN receive buffer {}",
                        dest_id, buffer_idx
                    );
                }
                new
            });
        }

        Ok(buffer_indices)
    }

    fn ensure_output_stream(&self, device_id: &str) -> RouteResult<()> {
        let starter_guard = self.output_stream_starter.read();
        let Some(starter) = starter_guard.as_ref() else {
            // No starter configured - this is OK during initialization or when not needed
            info!(
                "Output stream starter not configured, skipping output stream start for {}",
                device_id
            );
            return Ok(());
        };

        match starter(device_id) {
            Ok(()) => {
                info!("Output stream ensured for device: {}", device_id);
                Ok(())
            }
            Err(e) => {
                // If already running, that's fine
                let err_str = e.to_string();
                if err_str.contains("already exists") || err_str.contains("already running") {
                    info!("Output stream already running for device: {}", device_id);
                    Ok(())
                } else {
                    Err(RouteError::Internal(format!(
                        "Failed to start output stream for {}: {}",
                        device_id, e
                    )))
                }
            }
        }
    }

    fn all_input_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
        let contexts_guard = self.metering_contexts.read();
        if let Some(contexts) = contexts_guard.as_ref() {
            contexts.all_input_meters()
        } else {
            Vec::new()
        }
    }

    fn all_output_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
        let contexts_guard = self.metering_contexts.read();
        if let Some(contexts) = contexts_guard.as_ref() {
            contexts.all_output_meters()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_manager_creation() {
        let config = RouteManagerConfig::default();
        let manager = RouteManager::new(config);

        assert_eq!(manager.buffer_pool.free_count(), 256);
        assert_eq!(manager.stream_registry.input_count(), 0);
    }

    #[test]
    fn add_remove_route() {
        let config = RouteManagerConfig::default();
        let manager = RouteManager::new(config);

        let conn_id = ConnectionId::new("LOCAL", "input-device", 1, "LOCAL", "output-device", 1);

        let initial_free_count = manager.buffer_pool.free_count();

        // Add route
        let buffer_idx = manager.add_route_internal(conn_id.clone());
        assert!(buffer_idx.is_ok());

        let idx = buffer_idx.unwrap();
        assert!(manager.buffer_pool.is_allocated(idx));
        assert!(manager.connection_buffers.read().contains(&conn_id));
        assert_eq!(manager.buffer_pool.free_count(), initial_free_count - 1);

        // Remove route - buffer is now pending deferred free, not immediately freed
        let result = manager.remove_route_internal(&conn_id);
        assert!(result.is_ok());
        // Buffer is still marked as allocated (pending deferred free)
        assert!(manager.buffer_pool.is_allocated(idx));
        assert_eq!(manager.buffer_pool.pending_free_count(), 1);
        // Free count hasn't increased yet because buffer is pending
        assert_eq!(manager.buffer_pool.free_count(), initial_free_count - 1);

        // After enough generations pass, the buffer will be freed.
        // We need to call process_pending_frees with a high enough generation.
        // The buffer was deferred at generation 2 (after one add + one remove).
        // We need generation > 2 + 2 = 4 to actually free it.
        // Manually bump the generation by calling update_with multiple times.
        for _ in 0..3 {
            manager.routing_table.update_with(|current| current.clone());
        }
        // Now process pending frees with current generation (should be 5)
        let current_gen = manager.routing_table.generation();
        assert!(current_gen >= 5, "Generation should be >= 5, got {}", current_gen);
        manager.buffer_pool.process_pending_frees(current_gen);

        // Buffer should now be freed
        assert!(!manager.buffer_pool.is_allocated(idx));
        assert_eq!(manager.buffer_pool.pending_free_count(), 0);
        assert_eq!(manager.buffer_pool.free_count(), initial_free_count);
    }

    #[test]
    fn route_controller_trait() {
        let config = RouteManagerConfig::default();
        let manager = RouteManager::new(config);

        let conn_id = ConnectionId::new("LOCAL", "in", 1, "LOCAL", "out", 1);

        // Test RouteController trait methods
        assert!(manager.add_route(conn_id.clone()).is_ok());
        assert!(manager.has_route(&conn_id));
        assert!(manager.set_route_gain(&conn_id, 0.5).is_ok());
        assert!(manager.set_route_muted(&conn_id, true).is_ok());
        assert!(manager.remove_route(&conn_id).is_ok());
        assert!(!manager.has_route(&conn_id));
    }

    #[test]
    fn latency_calculation_local() {
        let config = RouteManagerConfig::default();
        let manager = RouteManager::new(config);

        let conn_id = ConnectionId::new("LOCAL", "in", 1, "LOCAL", "out", 1);

        let report = manager.calculate_latency(&conn_id);
        assert!(report.is_local());
        assert!(report.total_ms > 0.0);
        assert_eq!(report.network_ms, 0.0);
    }

    #[test]
    fn latency_calculation_network() {
        let config = RouteManagerConfig::default();
        let manager = RouteManager::new(config);

        let conn_id = ConnectionId::new("remote-node", "in", 1, "LOCAL", "out", 1);

        let report = manager.calculate_latency(&conn_id);
        assert!(!report.is_local());
        assert!(report.network_ms > 0.0);
    }
}
