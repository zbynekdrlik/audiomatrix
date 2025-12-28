//! Audio processor coordinator for managing real-time audio routing.
//!
//! The `AudioProcessor` is the central coordinator for audio processing. It owns:
//! - `RoutingTable`: The RCU-based routing configuration
//! - `RingBufferPool`: Pre-allocated buffers for audio data
//! - `StreamRegistry`: Active input/output streams
//!
//! It provides methods for:
//! - Starting/stopping audio streams
//! - Adding/removing routes
//! - Managing the audio processing lifecycle

// Allow dead code for now - some methods will be used when full wiring is complete
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig as CpalStreamConfig};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

// These imports will be used for stream lifecycle management
#[allow(unused_imports)]
use ram_core::active_stream::{ActiveInputStream, ActiveOutputStream, StreamConfig};
use ram_core::callbacks::{
    create_input_callback, create_output_callback, InputCallbackContext, OutputCallbackContext,
};
use ram_core::device::DeviceDirection;
use ram_core::latency::{LatencyCalculator, LatencyReport};
use ram_core::ring_buffer_pool::RingBufferPool;
use ram_core::route_controller::{RouteController, RouteError, RouteResult};
use ram_core::routing_snapshot::{DestinationSnapshot, RoutingSnapshot};
use ram_core::routing_table::RoutingTable;
use ram_core::stream_registry::StreamRegistry;
use ram_core::subscription_manager::SubscriptionManager;
use ram_core::ConnectionId;

/// Wrapper for a cpal Stream with its associated metadata.
struct CpalStreamHandle {
    /// The cpal stream (must be kept alive for audio to flow).
    stream: Stream,
    /// Device ID this stream belongs to.
    device_id: String,
    /// Number of channels.
    channels: u16,
    /// Sample rate.
    sample_rate: u32,
}

impl std::fmt::Debug for CpalStreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalStreamHandle")
            .field("device_id", &self.device_id)
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

/// Configuration for the audio processor.
#[derive(Debug, Clone)]
pub struct AudioProcessorConfig {
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

impl Default for AudioProcessorConfig {
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
struct ConnectionBufferMap {
    /// Connection ID -> buffer pool index
    connections: HashMap<ConnectionId, usize>,
}

impl ConnectionBufferMap {
    fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, conn_id: ConnectionId, buffer_index: usize) {
        self.connections.insert(conn_id, buffer_index);
    }

    fn remove(&mut self, conn_id: &ConnectionId) -> Option<usize> {
        self.connections.remove(conn_id)
    }

    fn get(&self, conn_id: &ConnectionId) -> Option<usize> {
        self.connections.get(conn_id).copied()
    }

    fn contains(&self, conn_id: &ConnectionId) -> bool {
        self.connections.contains_key(conn_id)
    }
}

/// Route manager that implements RouteController.
///
/// This struct contains only the thread-safe parts of AudioProcessor
/// that can be shared across threads for API access.
pub struct RouteManager {
    /// Configuration.
    config: AudioProcessorConfig,
    /// The RCU routing table.
    routing_table: Arc<RoutingTable>,
    /// Pre-allocated ring buffer pool.
    buffer_pool: Arc<RingBufferPool>,
    /// Active stream registry.
    stream_registry: Arc<StreamRegistry>,
    /// Subscription manager for cross-node routing.
    subscription_manager: Arc<SubscriptionManager>,
    /// Latency calculator.
    latency_calculator: LatencyCalculator,
    /// Connection ID to buffer index mapping.
    connection_buffers: RwLock<ConnectionBufferMap>,
    /// Destination ID to snapshot index mapping.
    dest_indices: RwLock<HashMap<String, usize>>,
}

impl RouteManager {
    /// Creates a new route manager with the given configuration.
    fn new(config: AudioProcessorConfig) -> Self {
        let buffer_pool = Arc::new(RingBufferPool::new(
            config.buffer_pool_size,
            config.buffer_capacity,
        ));
        let routing_table = Arc::new(RoutingTable::new());
        let stream_registry = Arc::new(StreamRegistry::new());
        let subscription_manager = Arc::new(SubscriptionManager::with_defaults(
            config.node_name.clone(),
        ));
        let latency_calculator = LatencyCalculator::new(config.default_sample_rate);

        Self {
            config,
            routing_table,
            buffer_pool,
            stream_registry,
            subscription_manager,
            latency_calculator,
            connection_buffers: RwLock::new(ConnectionBufferMap::new()),
            dest_indices: RwLock::new(HashMap::new()),
        }
    }

    /// Adds a route to the routing matrix.
    fn add_route_internal(&self, conn_id: ConnectionId) -> Result<usize> {
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
                let device_id = if parts.len() >= 2 { parts[1] } else { "unknown" };
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
    fn remove_route_internal(&self, conn_id: &ConnectionId) -> Result<()> {
        // Get and remove the buffer index
        let buffer_idx = self
            .connection_buffers
            .write()
            .remove(conn_id)
            .ok_or_else(|| anyhow!("Connection not found: {conn_id}"))?;

        // Update the routing table
        let dest_id = conn_id.destination_id();
        self.routing_table.update_with(|current| {
            let mut new = current.clone();
            if let Some(dest) = new.find_destination_mut(&dest_id) {
                dest.remove_source(buffer_idx);
            }
            new
        });

        // Free the buffer
        self.buffer_pool.free(buffer_idx);

        info!("Removed route {conn_id}, freed buffer index {buffer_idx}");
        Ok(())
    }

    /// Sets the gain for a connection.
    fn set_connection_gain_internal(&self, conn_id: &ConnectionId, gain: f32) -> Result<()> {
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
    fn set_connection_muted_internal(&self, conn_id: &ConnectionId, muted: bool) -> Result<()> {
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
    fn calculate_route_latency_internal(&self, conn_id: &ConnectionId) -> LatencyReport {
        let input_buffer = self.config.default_buffer_size;
        let ring_buffer = self.config.buffer_capacity as u32;
        let output_buffer = self.config.default_buffer_size;

        // Check if this is a cross-node route
        let local = &self.config.node_name;
        let source_is_local = conn_id.source_node == "LOCAL" || conn_id.source_node == *local;
        let dest_is_local = conn_id.destination_node == "LOCAL" || conn_id.destination_node == *local;
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
            self.latency_calculator.local_latency(input_buffer, ring_buffer, output_buffer)
        }
    }
}

/// Central coordinator for audio processing.
///
/// This struct owns all the components needed for lock-free audio routing:
/// - The routing table (RCU pattern)
/// - The ring buffer pool (pre-allocated)
/// - The stream registry (active streams)
///
/// # Thread Safety
///
/// - Audio callbacks read from RoutingTable and RingBufferPool lock-free
/// - Control operations (add/remove routes, start/stop streams) acquire locks
/// - The separation ensures audio never blocks on control operations
pub struct AudioProcessor {
    /// Thread-safe route manager (implements RouteController).
    route_manager: Arc<RouteManager>,
    /// Whether the processor is running.
    running: std::sync::atomic::AtomicBool,
    /// Active cpal input streams (device_id -> handle).
    input_streams: RwLock<HashMap<String, CpalStreamHandle>>,
    /// Active cpal output streams (device_id -> handle).
    output_streams: RwLock<HashMap<String, CpalStreamHandle>>,
}

impl std::fmt::Debug for AudioProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioProcessor")
            .field("config", &self.route_manager.config)
            .field("running", &self.running)
            .field("input_streams", &self.input_streams.read().len())
            .field("output_streams", &self.output_streams.read().len())
            .finish()
    }
}

impl AudioProcessor {
    /// Creates a new audio processor with the given configuration.
    #[must_use]
    pub fn new(config: AudioProcessorConfig) -> Self {
        info!(
            "AudioProcessor created: {} buffers x {} samples, node={}",
            config.buffer_pool_size, config.buffer_capacity, config.node_name
        );

        let route_manager = Arc::new(RouteManager::new(config));

        Self {
            route_manager,
            running: std::sync::atomic::AtomicBool::new(false),
            input_streams: RwLock::new(HashMap::new()),
            output_streams: RwLock::new(HashMap::new()),
        }
    }

    /// Creates an audio processor with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(AudioProcessorConfig::default())
    }

    /// Returns the route manager as a RouteController for API integration.
    #[must_use]
    pub fn route_controller(&self) -> Arc<dyn RouteController> {
        Arc::clone(&self.route_manager) as Arc<dyn RouteController>
    }

    /// Returns the routing table for read access.
    #[must_use]
    pub fn routing_table(&self) -> &Arc<RoutingTable> {
        &self.route_manager.routing_table
    }

    /// Returns the buffer pool.
    #[must_use]
    pub fn buffer_pool(&self) -> &Arc<RingBufferPool> {
        &self.route_manager.buffer_pool
    }

    /// Returns the stream registry.
    #[must_use]
    pub fn stream_registry(&self) -> &Arc<StreamRegistry> {
        &self.route_manager.stream_registry
    }

    /// Returns the subscription manager for cross-node routing.
    #[must_use]
    pub fn subscription_manager(&self) -> &Arc<SubscriptionManager> {
        &self.route_manager.subscription_manager
    }

    /// Returns the latency calculator.
    #[must_use]
    pub fn latency_calculator(&self) -> &LatencyCalculator {
        &self.route_manager.latency_calculator
    }

    /// Returns the local node name.
    #[must_use]
    pub fn node_name(&self) -> &str {
        &self.route_manager.config.node_name
    }

    /// Checks if a connection crosses node boundaries.
    ///
    /// A cross-node route has a source or destination node that differs
    /// from the local node name. This requires VBAN streaming.
    #[must_use]
    pub fn is_cross_node(&self, conn_id: &ConnectionId) -> bool {
        let local = &self.route_manager.config.node_name;

        // LOCAL is a placeholder for the current node
        let source_is_local = conn_id.source_node == "LOCAL" || conn_id.source_node == *local;
        let dest_is_local = conn_id.destination_node == "LOCAL" || conn_id.destination_node == *local;

        !source_is_local || !dest_is_local
    }

    /// Calculates expected latency for a route.
    ///
    /// Returns a latency report with breakdown of latency sources.
    #[must_use]
    pub fn calculate_route_latency(&self, conn_id: &ConnectionId) -> ram_core::latency::LatencyReport {
        let is_cross_node = self.is_cross_node(conn_id);
        let input_buffer = self.route_manager.config.default_buffer_size;
        let ring_buffer = self.route_manager.config.buffer_capacity as u32;
        let output_buffer = self.route_manager.config.default_buffer_size;

        if is_cross_node {
            // Network route with estimated 0.5ms RTT and 512 sample jitter buffer
            self.route_manager.latency_calculator.network_latency(
                input_buffer,
                ring_buffer,
                512, // jitter buffer samples
                0.5, // estimated LAN RTT in ms
                output_buffer,
            )
        } else {
            // Local route
            self.route_manager.latency_calculator.local_latency(
                input_buffer,
                ring_buffer,
                output_buffer,
            )
        }
    }

    /// Returns whether the processor is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Starts the audio processor.
    pub fn start(&self) {
        self.running
            .store(true, std::sync::atomic::Ordering::Release);
        info!("AudioProcessor started");
    }

    /// Stops the audio processor and all streams.
    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Release);

        // Stop all cpal streams (drop them to stop playback)
        {
            let mut input_streams = self.input_streams.write();
            let input_count = input_streams.len();
            input_streams.clear();
            info!("Stopped {} input streams", input_count);
        }
        {
            let mut output_streams = self.output_streams.write();
            let output_count = output_streams.len();
            output_streams.clear();
            info!("Stopped {} output streams", output_count);
        }

        // Stop all streams in registry
        self.stream_registry().clear();

        // Free all buffers
        self.route_manager.buffer_pool.free_all();

        // Clear routing
        self.route_manager.routing_table.update(RoutingSnapshot::new());

        info!("AudioProcessor stopped");
    }

    /// Creates an input stream context for a device.
    ///
    /// This allocates ring buffers for each channel and creates the callback context.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device identifier
    /// * `channel_count` - Number of input channels
    ///
    /// # Returns
    ///
    /// The input callback context and the buffer indices, or an error if buffer
    /// allocation failed.
    pub fn create_input_context(
        &self,
        device_id: &str,
        channel_count: usize,
    ) -> Result<(Arc<InputCallbackContext>, Vec<usize>)> {
        let mut buffer_indices = Vec::with_capacity(channel_count);

        // Allocate a buffer for each channel
        for ch in 0..channel_count {
            match self.route_manager.buffer_pool.allocate() {
                Some(idx) => buffer_indices.push(idx),
                None => {
                    // Free any buffers we've allocated so far
                    for &idx in &buffer_indices {
                        self.route_manager.buffer_pool.free(idx);
                    }
                    return Err(anyhow!(
                        "Failed to allocate buffer for channel {ch} on device {device_id}: pool exhausted"
                    ));
                }
            }
        }

        let context = Arc::new(InputCallbackContext::new(
            Arc::clone(&self.route_manager.buffer_pool),
            buffer_indices.clone(),
            device_id,
        ));

        debug!(
            "Created input context for {device_id}: {} channels, buffers {:?}",
            channel_count, buffer_indices
        );

        Ok((context, buffer_indices))
    }

    /// Creates an output stream context for a device.
    ///
    /// This looks up the destination indices in the routing snapshot and creates
    /// the callback context.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device identifier
    /// * `channel_count` - Number of output channels
    ///
    /// # Returns
    ///
    /// The output callback context and the destination indices.
    pub fn create_output_context(
        &self,
        device_id: &str,
        channel_count: usize,
    ) -> (Arc<OutputCallbackContext>, Vec<usize>) {
        // Get or create destination indices for this device's channels
        let dest_indices: Vec<usize> = (0..channel_count)
            .map(|ch| self.get_or_create_dest_index(device_id, ch))
            .collect();

        let context = Arc::new(OutputCallbackContext::new(
            Arc::clone(&self.route_manager.routing_table),
            Arc::clone(&self.route_manager.buffer_pool),
            dest_indices.clone(),
            device_id,
        ));

        debug!(
            "Created output context for {device_id}: {} channels, dest indices {:?}",
            channel_count, dest_indices
        );

        (context, dest_indices)
    }

    /// Gets or creates a destination index for a device channel.
    fn get_or_create_dest_index(&self, device_id: &str, channel: usize) -> usize {
        let dest_id = format!("LOCAL:{device_id}:{}", channel + 1);

        // Check if we already have this destination
        {
            let indices = self.route_manager.dest_indices.read();
            if let Some(&idx) = indices.get(&dest_id) {
                return idx;
            }
        }

        // Create new destination in routing table
        self.route_manager.routing_table.update_with(|current| {
            let mut new = current.clone();

            // Check if it was created by another thread
            if let Some(_existing_idx) = new
                .destinations
                .iter()
                .position(|d| d.dest_id == dest_id)
            {
                // Another thread created it, use that index
                return new;
            }

            // Create new destination
            let dest = DestinationSnapshot::new(
                dest_id.clone(),
                device_id.to_string(),
                channel,
            );
            new.destinations.push(dest);
            new
        });

        // Get the index of the destination we just created
        let snapshot = self.route_manager.routing_table.snapshot();
        let idx = snapshot
            .destinations
            .iter()
            .position(|d| d.dest_id == dest_id)
            .unwrap_or(0);

        // Cache the index
        self.route_manager.dest_indices.write().insert(dest_id, idx);

        idx
    }

    /// Adds a route (connection) from a source to a destination.
    ///
    /// This allocates a ring buffer for the connection and updates the routing table.
    ///
    /// # Arguments
    ///
    /// * `conn_id` - The connection identifier
    ///
    /// # Returns
    ///
    /// The buffer index allocated for this connection, or an error.
    pub fn add_route(&self, conn_id: ConnectionId) -> Result<usize> {
        // Check if connection already exists
        if self.route_manager.connection_buffers.read().contains(&conn_id) {
            return Err(anyhow!("Connection already exists: {conn_id}"));
        }

        // Allocate a buffer for this connection
        let buffer_idx = self
            .route_manager.buffer_pool
            .allocate()
            .ok_or_else(|| anyhow!("Buffer pool exhausted"))?;

        // Store the mapping
        self.route_manager.connection_buffers
            .write()
            .insert(conn_id.clone(), buffer_idx);

        // Update the routing table
        let dest_id = conn_id.destination_id();
        self.route_manager.routing_table.update_with(|current| {
            let mut new = current.clone();
            if let Some(dest) = new.find_destination_mut(&dest_id) {
                dest.add_source(buffer_idx);
            } else {
                // Create destination if it doesn't exist
                let parts: Vec<&str> = dest_id.split(':').collect();
                let device_id = if parts.len() >= 2 { parts[1] } else { "unknown" };
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

    /// Removes a route (connection).
    ///
    /// This frees the ring buffer and updates the routing table.
    ///
    /// # Arguments
    ///
    /// * `conn_id` - The connection identifier
    ///
    /// # Returns
    ///
    /// Ok if the route was removed, error if it didn't exist.
    pub fn remove_route(&self, conn_id: &ConnectionId) -> Result<()> {
        self.route_manager.remove_route_internal(conn_id)
    }

    /// Gets the buffer index for a connection.
    #[must_use]
    pub fn get_connection_buffer(&self, conn_id: &ConnectionId) -> Option<usize> {
        self.route_manager.connection_buffers.read().get(conn_id)
    }

    /// Sets the gain for a connection.
    ///
    /// # Arguments
    ///
    /// * `conn_id` - The connection identifier
    /// * `gain` - The gain value (0.0 to ~4.0)
    pub fn set_connection_gain(&self, conn_id: &ConnectionId, gain: f32) -> Result<()> {
        self.route_manager.set_connection_gain_internal(conn_id, gain)
    }

    /// Sets the mute state for a connection.
    ///
    /// # Arguments
    ///
    /// * `conn_id` - The connection identifier
    /// * `muted` - Whether the connection is muted
    pub fn set_connection_muted(&self, conn_id: &ConnectionId, muted: bool) -> Result<()> {
        self.route_manager.set_connection_muted_internal(conn_id, muted)
    }

    /// Gets statistics about the audio processor.
    #[must_use]
    pub fn stats(&self) -> AudioProcessorStats {
        let input_stats = self.stream_registry().input_stats();
        let output_stats = self.stream_registry().output_stats();

        AudioProcessorStats {
            is_running: self.is_running(),
            active_input_streams: self.stream_registry().input_count(),
            active_output_streams: self.stream_registry().output_count(),
            allocated_buffers: self.route_manager.buffer_pool.allocated_count(),
            free_buffers: self.route_manager.buffer_pool.free_count(),
            active_connections: self.route_manager.connection_buffers.read().connections.len(),
            routing_generation: self.route_manager.routing_table.generation(),
            total_input_callbacks: input_stats.total_callbacks,
            total_output_callbacks: output_stats.total_callbacks,
            total_underruns: input_stats.total_underruns + output_stats.total_underruns,
            total_overruns: input_stats.total_overruns + output_stats.total_overruns,
        }
    }

    // ========================================================================
    // Stream Management
    // ========================================================================

    /// Starts an input stream for the specified device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device ID in the format "host:direction:name"
    ///
    /// # Returns
    ///
    /// Ok(()) if the stream was started, error otherwise.
    pub fn start_input_stream(&self, device_id: &str) -> Result<()> {
        // Check if stream already exists
        if self.input_streams.read().contains_key(device_id) {
            return Err(anyhow!("Input stream already exists for device: {device_id}"));
        }

        // Find the cpal device
        let (device, config) = Self::find_cpal_device(device_id, DeviceDirection::Input)?;

        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        // Create input context with buffers for each channel
        let (context, buffer_indices) = self.create_input_context(device_id, channels as usize)?;

        info!(
            "Starting input stream on {}: {} channels @ {}Hz, buffers {:?}",
            device_id, channels, sample_rate, buffer_indices
        );

        // Create the stream
        let stream = Self::build_input_stream(&device, &config, context)?;

        // Start the stream
        stream.play().map_err(|e| anyhow!("Failed to start input stream: {e}"))?;

        // Store the stream handle
        self.input_streams.write().insert(
            device_id.to_string(),
            CpalStreamHandle {
                stream,
                device_id: device_id.to_string(),
                channels,
                sample_rate,
            },
        );

        info!("Input stream started on {device_id}");
        Ok(())
    }

    /// Starts an output stream for the specified device.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device ID in the format "host:direction:name"
    ///
    /// # Returns
    ///
    /// Ok(()) if the stream was started, error otherwise.
    pub fn start_output_stream(&self, device_id: &str) -> Result<()> {
        // Check if stream already exists
        if self.output_streams.read().contains_key(device_id) {
            return Err(anyhow!("Output stream already exists for device: {device_id}"));
        }

        // Find the cpal device
        let (device, config) = Self::find_cpal_device(device_id, DeviceDirection::Output)?;

        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        // Create output context with destination indices
        let (context, dest_indices) = self.create_output_context(device_id, channels as usize);

        info!(
            "Starting output stream on {}: {} channels @ {}Hz, destinations {:?}",
            device_id, channels, sample_rate, dest_indices
        );

        // Create the stream
        let stream = Self::build_output_stream(&device, &config, context)?;

        // Start the stream
        stream.play().map_err(|e| anyhow!("Failed to start output stream: {e}"))?;

        // Store the stream handle
        self.output_streams.write().insert(
            device_id.to_string(),
            CpalStreamHandle {
                stream,
                device_id: device_id.to_string(),
                channels,
                sample_rate,
            },
        );

        info!("Output stream started on {device_id}");
        Ok(())
    }

    /// Stops an input stream for the specified device.
    pub fn stop_input_stream(&self, device_id: &str) -> Result<()> {
        let removed = self.input_streams.write().remove(device_id);
        if removed.is_some() {
            info!("Input stream stopped on {device_id}");
            Ok(())
        } else {
            Err(anyhow!("No input stream found for device: {device_id}"))
        }
    }

    /// Stops an output stream for the specified device.
    pub fn stop_output_stream(&self, device_id: &str) -> Result<()> {
        let removed = self.output_streams.write().remove(device_id);
        if removed.is_some() {
            info!("Output stream stopped on {device_id}");
            Ok(())
        } else {
            Err(anyhow!("No output stream found for device: {device_id}"))
        }
    }

    /// Returns the number of active input streams.
    #[must_use]
    pub fn active_input_stream_count(&self) -> usize {
        self.input_streams.read().len()
    }

    /// Returns the number of active output streams.
    #[must_use]
    pub fn active_output_stream_count(&self) -> usize {
        self.output_streams.read().len()
    }

    // ========================================================================
    // Private Helpers
    // ========================================================================

    /// Finds a cpal device by ID and returns it with a suitable stream config.
    fn find_cpal_device(
        device_id: &str,
        direction: DeviceDirection,
    ) -> Result<(cpal::Device, CpalStreamConfig)> {
        // Parse device ID: "host:direction:name"
        let parts: Vec<&str> = device_id.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(anyhow!("Invalid device ID format: {device_id}"));
        }
        let host_name = parts[0];
        let device_name = parts[2];

        // Find the host
        let hosts = cpal::available_hosts();
        let host_id = hosts
            .iter()
            .find(|h| h.name() == host_name)
            .ok_or_else(|| anyhow!("Host not found: {host_name}"))?;

        let host = cpal::host_from_id(*host_id)
            .map_err(|e| anyhow!("Failed to get host {host_name}: {e}"))?;

        // Find the device
        let device = match direction {
            DeviceDirection::Input => {
                host.input_devices()
                    .map_err(|e| anyhow!("Failed to enumerate input devices: {e}"))?
                    .find(|d| d.name().ok().as_deref() == Some(device_name))
                    .ok_or_else(|| anyhow!("Input device not found: {device_name}"))?
            }
            DeviceDirection::Output => {
                host.output_devices()
                    .map_err(|e| anyhow!("Failed to enumerate output devices: {e}"))?
                    .find(|d| d.name().ok().as_deref() == Some(device_name))
                    .ok_or_else(|| anyhow!("Output device not found: {device_name}"))?
            }
        };

        // Get default config
        let config = match direction {
            DeviceDirection::Input => device
                .default_input_config()
                .map_err(|e| anyhow!("Failed to get input config: {e}"))?,
            DeviceDirection::Output => device
                .default_output_config()
                .map_err(|e| anyhow!("Failed to get output config: {e}"))?,
        };

        // Convert to StreamConfig
        let stream_config = CpalStreamConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        Ok((device, stream_config))
    }

    /// Builds a cpal input stream with our callback.
    fn build_input_stream(
        device: &cpal::Device,
        config: &CpalStreamConfig,
        context: Arc<InputCallbackContext>,
    ) -> Result<Stream> {
        let err_fn = |err| error!("Input stream error: {err}");

        // Create our callback and wrap it to accept cpal's CallbackInfo
        let mut inner_callback = create_input_callback(context);
        let callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            inner_callback(data);
        };

        let stream = device
            .build_input_stream(config, callback, err_fn, None)
            .map_err(|e| anyhow!("Failed to build input stream: {e}"))?;

        Ok(stream)
    }

    /// Builds a cpal output stream with our callback.
    fn build_output_stream(
        device: &cpal::Device,
        config: &CpalStreamConfig,
        context: Arc<OutputCallbackContext>,
    ) -> Result<Stream> {
        let err_fn = |err| error!("Output stream error: {err}");

        // Create our callback and wrap it to accept cpal's CallbackInfo
        let mut inner_callback = create_output_callback(context);
        let callback = move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
            inner_callback(data);
        };

        let stream = device
            .build_output_stream(config, callback, err_fn, None)
            .map_err(|e| anyhow!("Failed to build output stream: {e}"))?;

        Ok(stream)
    }
}

/// Statistics about the audio processor.
#[derive(Debug, Clone, Default)]
pub struct AudioProcessorStats {
    /// Whether the processor is running.
    pub is_running: bool,
    /// Number of active input streams.
    pub active_input_streams: usize,
    /// Number of active output streams.
    pub active_output_streams: usize,
    /// Number of allocated buffers.
    pub allocated_buffers: usize,
    /// Number of free buffers.
    pub free_buffers: usize,
    /// Number of active connections.
    pub active_connections: usize,
    /// Current routing generation.
    pub routing_generation: u64,
    /// Total input callbacks processed.
    pub total_input_callbacks: u64,
    /// Total output callbacks processed.
    pub total_output_callbacks: u64,
    /// Total underruns.
    pub total_underruns: u64,
    /// Total overruns.
    pub total_overruns: u64,
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
        self.set_connection_gain_internal(conn_id, gain).map_err(|e| {
            if e.to_string().contains("not found") {
                RouteError::NotFound(conn_id.to_string())
            } else {
                RouteError::Internal(e.to_string())
            }
        })
    }

    fn set_route_muted(&self, conn_id: &ConnectionId, muted: bool) -> RouteResult<()> {
        self.set_connection_muted_internal(conn_id, muted).map_err(|e| {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_processor_creation() {
        let processor = AudioProcessor::with_defaults();
        assert!(!processor.is_running());
        assert_eq!(processor.buffer_pool().free_count(), 256);
    }

    #[test]
    fn audio_processor_start_stop() {
        let processor = AudioProcessor::with_defaults();

        assert!(!processor.is_running());
        processor.start();
        assert!(processor.is_running());
        processor.stop();
        assert!(!processor.is_running());
    }

    #[test]
    fn create_input_context() {
        let processor = AudioProcessor::with_defaults();

        let result = processor.create_input_context("device-1", 2);
        assert!(result.is_ok());

        let (context, indices) = result.unwrap();
        assert_eq!(context.channel_count(), 2);
        assert_eq!(indices.len(), 2);

        // Buffers should be allocated
        assert_eq!(processor.buffer_pool().allocated_count(), 2);
    }

    #[test]
    fn create_output_context() {
        let processor = AudioProcessor::with_defaults();

        let (context, indices) = processor.create_output_context("device-1", 2);
        assert_eq!(context.dest_indices().len(), 2);
        assert_eq!(indices.len(), 2);

        // Destinations should be created in routing table
        let snapshot = processor.routing_table().snapshot();
        assert_eq!(snapshot.destinations.len(), 2);
    }

    #[test]
    fn add_remove_route() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "input-device", 1,
            "LOCAL", "output-device", 1,
        );

        // Add route
        let buffer_idx = processor.add_route(conn_id.clone());
        assert!(buffer_idx.is_ok());

        let idx = buffer_idx.unwrap();
        assert!(processor.buffer_pool().is_allocated(idx));
        assert!(processor.get_connection_buffer(&conn_id).is_some());

        // Check routing table has the source
        let snapshot = processor.routing_table().snapshot();
        assert!(snapshot.destinations.iter().any(|d| d.active_count > 0));

        // Remove route
        let result = processor.remove_route(&conn_id);
        assert!(result.is_ok());
        assert!(!processor.buffer_pool().is_allocated(idx));
        assert!(processor.get_connection_buffer(&conn_id).is_none());
    }

    #[test]
    fn add_duplicate_route_fails() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        assert!(processor.add_route(conn_id.clone()).is_ok());
        assert!(processor.add_route(conn_id).is_err());
    }

    #[test]
    fn remove_nonexistent_route_fails() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        assert!(processor.remove_route(&conn_id).is_err());
    }

    #[test]
    fn set_connection_gain() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        processor.add_route(conn_id.clone()).unwrap();

        let result = processor.set_connection_gain(&conn_id, 0.5);
        assert!(result.is_ok());
    }

    #[test]
    fn set_connection_muted() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        processor.add_route(conn_id.clone()).unwrap();

        let result = processor.set_connection_muted(&conn_id, true);
        assert!(result.is_ok());
    }

    #[test]
    fn stats() {
        let processor = AudioProcessor::with_defaults();

        let stats = processor.stats();
        assert!(!stats.is_running);
        assert_eq!(stats.active_input_streams, 0);
        assert_eq!(stats.active_output_streams, 0);
        assert_eq!(stats.allocated_buffers, 0);
        assert_eq!(stats.free_buffers, 256);

        processor.start();
        let stats = processor.stats();
        assert!(stats.is_running);
    }

    #[test]
    fn is_cross_node_local() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        assert!(!processor.is_cross_node(&conn_id));
    }

    #[test]
    fn is_cross_node_remote_source() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "remote-node", "in", 1,
            "LOCAL", "out", 1,
        );

        assert!(processor.is_cross_node(&conn_id));
    }

    #[test]
    fn is_cross_node_remote_dest() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "remote-node", "out", 1,
        );

        assert!(processor.is_cross_node(&conn_id));
    }

    #[test]
    fn calculate_route_latency_local() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "LOCAL", "in", 1,
            "LOCAL", "out", 1,
        );

        let report = processor.calculate_route_latency(&conn_id);
        assert!(report.is_local());
        assert!(report.total_ms > 0.0);
        assert_eq!(report.network_ms, 0.0);
    }

    #[test]
    fn calculate_route_latency_network() {
        let processor = AudioProcessor::with_defaults();

        let conn_id = ConnectionId::new(
            "remote-node", "in", 1,
            "LOCAL", "out", 1,
        );

        let report = processor.calculate_route_latency(&conn_id);
        assert!(!report.is_local());
        assert!(report.network_ms > 0.0);
        assert!(report.total_ms > report.network_ms);
    }

    #[test]
    fn subscription_manager_accessible() {
        let processor = AudioProcessor::with_defaults();

        let manager = processor.subscription_manager();
        let stats = manager.stats();

        assert_eq!(stats.outgoing_total, 0);
        assert_eq!(stats.incoming_total, 0);
    }

    #[test]
    fn latency_calculator_accessible() {
        let processor = AudioProcessor::with_defaults();

        let calc = processor.latency_calculator();
        let latency = calc.buffer_latency_ms(256);

        // 256 samples @ 48kHz = ~5.33ms
        assert!(latency > 5.0 && latency < 6.0);
    }

    #[test]
    fn node_name_accessible() {
        let mut config = AudioProcessorConfig::default();
        config.node_name = "test-node".to_string();

        let processor = AudioProcessor::new(config);
        assert_eq!(processor.node_name(), "test-node");
    }
}
