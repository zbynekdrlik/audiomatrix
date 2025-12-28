//! Audio processor coordinator for managing real-time audio routing.
//!
//! The `AudioProcessor` is the central coordinator for audio processing. It owns:
//! - `RouteManager`: Route management and API integration
//! - Stream handles for active input/output devices
//!
//! It provides methods for:
//! - Starting/stopping audio streams
//! - Adding/removing routes (via RouteManager)
//! - Managing the audio processing lifecycle

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig as CpalStreamConfig};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

use ram_core::callbacks::{
    create_input_callback, create_output_callback, InputCallbackContext, OutputCallbackContext,
};
use ram_core::device::DeviceDirection;
use ram_core::latency::LatencyCalculator;
use ram_core::ring_buffer_pool::RingBufferPool;
use ram_core::route_controller::RouteController;
use ram_core::routing_snapshot::{DestinationSnapshot, RoutingSnapshot};
use ram_core::routing_table::RoutingTable;
use ram_core::stream_registry::StreamRegistry;
use ram_core::subscription_manager::SubscriptionManager;
use ram_core::ConnectionId;

use crate::route_manager::{RouteManager, RouteManagerConfig};

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

impl From<AudioProcessorConfig> for RouteManagerConfig {
    fn from(config: AudioProcessorConfig) -> Self {
        Self {
            buffer_pool_size: config.buffer_pool_size,
            buffer_capacity: config.buffer_capacity,
            default_sample_rate: config.default_sample_rate,
            default_buffer_size: config.default_buffer_size,
            node_name: config.node_name,
        }
    }
}

/// Central coordinator for audio processing.
///
/// This struct owns all the components needed for lock-free audio routing:
/// - The route manager (routing table, buffer pool)
/// - The stream handles (cpal streams)
///
/// # Thread Safety
///
/// - Audio callbacks read from RoutingTable and RingBufferPool lock-free
/// - Control operations (add/remove routes, start/stop streams) acquire locks
/// - The separation ensures audio never blocks on control operations
pub struct AudioProcessor {
    /// Thread-safe route manager (implements RouteController).
    route_manager: Arc<RouteManager>,
    /// Configuration.
    config: AudioProcessorConfig,
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
            .field("config", &self.config)
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

        let route_manager = Arc::new(RouteManager::new(config.clone().into()));

        Self {
            route_manager,
            config,
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
    #[must_use]
    pub fn is_cross_node(&self, conn_id: &ConnectionId) -> bool {
        let local = &self.route_manager.config.node_name;
        let source_is_local = conn_id.source_node == "LOCAL" || conn_id.source_node == *local;
        let dest_is_local =
            conn_id.destination_node == "LOCAL" || conn_id.destination_node == *local;
        !source_is_local || !dest_is_local
    }

    /// Calculates expected latency for a route.
    #[must_use]
    pub fn calculate_route_latency(
        &self,
        conn_id: &ConnectionId,
    ) -> ram_core::latency::LatencyReport {
        self.route_manager.calculate_route_latency_internal(conn_id)
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

        // Stop all cpal streams
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
        self.route_manager
            .routing_table
            .update(RoutingSnapshot::new());

        info!("AudioProcessor stopped");
    }

    /// Adds a route (connection) from a source to a destination.
    pub fn add_route(&self, conn_id: ConnectionId) -> Result<usize> {
        self.route_manager.add_route_internal(conn_id)
    }

    /// Removes a route (connection).
    pub fn remove_route(&self, conn_id: &ConnectionId) -> Result<()> {
        self.route_manager.remove_route_internal(conn_id)
    }

    /// Gets the buffer index for a connection.
    #[must_use]
    pub fn get_connection_buffer(&self, conn_id: &ConnectionId) -> Option<usize> {
        self.route_manager.connection_buffers.read().get(conn_id)
    }

    /// Sets the gain for a connection.
    pub fn set_connection_gain(&self, conn_id: &ConnectionId, gain: f32) -> Result<()> {
        self.route_manager
            .set_connection_gain_internal(conn_id, gain)
    }

    /// Sets the mute state for a connection.
    pub fn set_connection_muted(&self, conn_id: &ConnectionId, muted: bool) -> Result<()> {
        self.route_manager
            .set_connection_muted_internal(conn_id, muted)
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
            active_connections: self
                .route_manager
                .connection_buffers
                .read()
                .connections
                .len(),
            routing_generation: self.route_manager.routing_table.generation(),
            total_input_callbacks: input_stats.total_callbacks,
            total_output_callbacks: output_stats.total_callbacks,
            total_underruns: input_stats.total_underruns + output_stats.total_underruns,
            total_overruns: input_stats.total_overruns + output_stats.total_overruns,
        }
    }

    // ========================================================================
    // Stream Context Creation
    // ========================================================================

    /// Creates an input stream context for a device.
    pub fn create_input_context(
        &self,
        device_id: &str,
        channel_count: usize,
    ) -> Result<(Arc<InputCallbackContext>, Vec<usize>)> {
        let mut buffer_indices = Vec::with_capacity(channel_count);

        for ch in 0..channel_count {
            match self.route_manager.buffer_pool.allocate() {
                Some(idx) => buffer_indices.push(idx),
                None => {
                    for &idx in &buffer_indices {
                        self.route_manager.buffer_pool.free(idx);
                    }
                    return Err(anyhow!(
                        "Failed to allocate buffer for channel {ch} on device {device_id}: pool exhausted"
                    ));
                },
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
    pub fn create_output_context(
        &self,
        device_id: &str,
        channel_count: usize,
    ) -> (Arc<OutputCallbackContext>, Vec<usize>) {
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

        {
            let indices = self.route_manager.dest_indices.read();
            if let Some(&idx) = indices.get(&dest_id) {
                return idx;
            }
        }

        self.route_manager.routing_table.update_with(|current| {
            let mut new = current.clone();
            if new.destinations.iter().any(|d| d.dest_id == dest_id) {
                return new;
            }
            let dest = DestinationSnapshot::new(dest_id.clone(), device_id.to_string(), channel);
            new.destinations.push(dest);
            new
        });

        let snapshot = self.route_manager.routing_table.snapshot();
        let idx = snapshot
            .destinations
            .iter()
            .position(|d| d.dest_id == dest_id)
            .unwrap_or(0);

        self.route_manager.dest_indices.write().insert(dest_id, idx);
        idx
    }

    // ========================================================================
    // Stream Management
    // ========================================================================

    /// Starts an input stream for the specified device.
    pub fn start_input_stream(&self, device_id: &str) -> Result<()> {
        if self.input_streams.read().contains_key(device_id) {
            return Err(anyhow!(
                "Input stream already exists for device: {device_id}"
            ));
        }

        let (device, config) = Self::find_cpal_device(device_id, DeviceDirection::Input)?;
        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        let (context, buffer_indices) = self.create_input_context(device_id, channels as usize)?;

        info!(
            "Starting input stream on {}: {} channels @ {}Hz, buffers {:?}",
            device_id, channels, sample_rate, buffer_indices
        );

        let stream = Self::build_input_stream(&device, &config, context)?;
        stream
            .play()
            .map_err(|e| anyhow!("Failed to start input stream: {e}"))?;

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
    pub fn start_output_stream(&self, device_id: &str) -> Result<()> {
        if self.output_streams.read().contains_key(device_id) {
            return Err(anyhow!(
                "Output stream already exists for device: {device_id}"
            ));
        }

        let (device, config) = Self::find_cpal_device(device_id, DeviceDirection::Output)?;
        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        let (context, dest_indices) = self.create_output_context(device_id, channels as usize);

        info!(
            "Starting output stream on {}: {} channels @ {}Hz, destinations {:?}",
            device_id, channels, sample_rate, dest_indices
        );

        let stream = Self::build_output_stream(&device, &config, context)?;
        stream
            .play()
            .map_err(|e| anyhow!("Failed to start output stream: {e}"))?;

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

    fn find_cpal_device(
        device_id: &str,
        direction: DeviceDirection,
    ) -> Result<(cpal::Device, CpalStreamConfig)> {
        let parts: Vec<&str> = device_id.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(anyhow!("Invalid device ID format: {device_id}"));
        }
        let host_name = parts[0];
        let device_name = parts[2];

        let hosts = cpal::available_hosts();
        let host_id = hosts
            .iter()
            .find(|h| h.name() == host_name)
            .ok_or_else(|| anyhow!("Host not found: {host_name}"))?;

        let host = cpal::host_from_id(*host_id)
            .map_err(|e| anyhow!("Failed to get host {host_name}: {e}"))?;

        let device = match direction {
            DeviceDirection::Input => host
                .input_devices()
                .map_err(|e| anyhow!("Failed to enumerate input devices: {e}"))?
                .find(|d| d.name().ok().as_deref() == Some(device_name))
                .ok_or_else(|| anyhow!("Input device not found: {device_name}"))?,
            DeviceDirection::Output => host
                .output_devices()
                .map_err(|e| anyhow!("Failed to enumerate output devices: {e}"))?
                .find(|d| d.name().ok().as_deref() == Some(device_name))
                .ok_or_else(|| anyhow!("Output device not found: {device_name}"))?,
        };

        let config = match direction {
            DeviceDirection::Input => device
                .default_input_config()
                .map_err(|e| anyhow!("Failed to get input config: {e}"))?,
            DeviceDirection::Output => device
                .default_output_config()
                .map_err(|e| anyhow!("Failed to get output config: {e}"))?,
        };

        let stream_config = CpalStreamConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        Ok((device, stream_config))
    }

    fn build_input_stream(
        device: &cpal::Device,
        config: &CpalStreamConfig,
        context: Arc<InputCallbackContext>,
    ) -> Result<Stream> {
        let err_fn = |err| error!("Input stream error: {err}");

        let mut inner_callback = create_input_callback(context);
        let callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            inner_callback(data);
        };

        let stream = device
            .build_input_stream(config, callback, err_fn, None)
            .map_err(|e| anyhow!("Failed to build input stream: {e}"))?;

        Ok(stream)
    }

    fn build_output_stream(
        device: &cpal::Device,
        config: &CpalStreamConfig,
        context: Arc<OutputCallbackContext>,
    ) -> Result<Stream> {
        let err_fn = |err| error!("Output stream error: {err}");

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
    pub is_running: bool,
    pub active_input_streams: usize,
    pub active_output_streams: usize,
    pub allocated_buffers: usize,
    pub free_buffers: usize,
    pub active_connections: usize,
    pub routing_generation: u64,
    pub total_input_callbacks: u64,
    pub total_output_callbacks: u64,
    pub total_underruns: u64,
    pub total_overruns: u64,
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
        assert_eq!(processor.buffer_pool().allocated_count(), 2);
    }

    #[test]
    fn create_output_context() {
        let processor = AudioProcessor::with_defaults();
        let (context, indices) = processor.create_output_context("device-1", 2);
        assert_eq!(context.dest_indices().len(), 2);
        assert_eq!(indices.len(), 2);

        let snapshot = processor.routing_table().snapshot();
        assert_eq!(snapshot.destinations.len(), 2);
    }

    #[test]
    fn add_remove_route() {
        let processor = AudioProcessor::with_defaults();
        let conn_id = ConnectionId::new("LOCAL", "input-device", 1, "LOCAL", "output-device", 1);

        let initial_free_count = processor.buffer_pool().free_count();

        let buffer_idx = processor.add_route(conn_id.clone());
        assert!(buffer_idx.is_ok());

        let idx = buffer_idx.unwrap();
        assert!(processor.buffer_pool().is_allocated(idx));
        assert!(processor.get_connection_buffer(&conn_id).is_some());
        assert_eq!(processor.buffer_pool().free_count(), initial_free_count - 1);

        // Remove route - buffer is pending deferred free, not immediately freed
        let result = processor.remove_route(&conn_id);
        assert!(result.is_ok());
        // Buffer is still marked as allocated (pending deferred free)
        assert!(processor.buffer_pool().is_allocated(idx));
        assert_eq!(processor.buffer_pool().pending_free_count(), 1);

        // Bump generation and process pending frees to actually free the buffer
        for _ in 0..3 {
            processor.routing_table().update_with(|current| current.clone());
        }
        let current_gen = processor.routing_table().generation();
        processor.buffer_pool().process_pending_frees(current_gen);

        // Now the buffer should be freed
        assert!(!processor.buffer_pool().is_allocated(idx));
        assert_eq!(processor.buffer_pool().free_count(), initial_free_count);
    }

    #[test]
    fn stats() {
        let processor = AudioProcessor::with_defaults();
        let stats = processor.stats();
        assert!(!stats.is_running);
        assert_eq!(stats.free_buffers, 256);

        processor.start();
        let stats = processor.stats();
        assert!(stats.is_running);
    }

    #[test]
    fn is_cross_node() {
        let processor = AudioProcessor::with_defaults();
        let local = ConnectionId::new("LOCAL", "in", 1, "LOCAL", "out", 1);
        let remote = ConnectionId::new("remote-node", "in", 1, "LOCAL", "out", 1);

        assert!(!processor.is_cross_node(&local));
        assert!(processor.is_cross_node(&remote));
    }
}
