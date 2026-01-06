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
use cpal::traits::StreamTrait;
use cpal::Stream;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use ram_core::active_stream::{ActiveInputStream, ActiveOutputStream, StreamConfig};
use ram_core::callbacks::{InputCallbackContext, OutputCallbackContext};

use crate::stream_helpers::{
    build_input_stream, build_output_stream, find_cpal_device, DeviceDirection,
};
use ram_core::latency::LatencyCalculator;
use ram_core::metering::MeterLevels;
use ram_core::ring_buffer_pool::RingBufferPool;
use ram_core::route_controller::RouteController;
use ram_core::routing_snapshot::{DestinationSnapshot, RoutingSnapshot};
use ram_core::routing_table::RoutingTable;
use ram_core::stream_registry::StreamRegistry;
use ram_core::subscription_manager::SubscriptionManager;
use ram_core::ConnectionId;

use crate::route_manager::{RouteManager, RouteManagerConfig};

/// Wrapper to make cpal::Stream implement Sync.
///
/// SAFETY: This is safe because:
/// 1. The Stream is only accessed through RwLock, ensuring mutual exclusion
/// 2. We never share raw references to Stream across threads
/// 3. All operations (play/pause/drop) happen on the owning thread
struct SyncStream(Stream);

// SAFETY: See SyncStream doc comment
unsafe impl Sync for SyncStream {}
unsafe impl Send for SyncStream {}

/// Wrapper for a cpal stream with its associated metadata.
struct CpalStreamHandle {
    /// The cpal stream (must be kept alive for audio to flow).
    #[allow(dead_code)]
    stream: SyncStream,
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

/// Thread-safe container for metering contexts.
/// Separated from cpal streams to allow cross-thread access.
#[derive(Debug, Default)]
pub struct MeteringContexts {
    /// Input device contexts (device_id -> context).
    input: RwLock<HashMap<String, Arc<InputCallbackContext>>>,
    /// Output device contexts (device_id -> context).
    output: RwLock<HashMap<String, Arc<OutputCallbackContext>>>,
}

impl MeteringContexts {
    /// Creates a new empty metering contexts container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an input context.
    pub fn register_input(&self, device_id: &str, context: Arc<InputCallbackContext>) {
        self.input.write().insert(device_id.to_string(), context);
    }

    /// Gets buffer indices for an input device if it's registered.
    pub fn get_input_buffer_indices(&self, device_id: &str) -> Option<Vec<usize>> {
        self.input
            .read()
            .get(device_id)
            .map(|ctx| ctx.buffer_indices().to_vec())
    }

    /// Registers an output context.
    pub fn register_output(&self, device_id: &str, context: Arc<OutputCallbackContext>) {
        self.output.write().insert(device_id.to_string(), context);
    }

    /// Unregisters an input context.
    pub fn unregister_input(&self, device_id: &str) {
        self.input.write().remove(device_id);
    }

    /// Unregisters an output context.
    pub fn unregister_output(&self, device_id: &str) {
        self.output.write().remove(device_id);
    }

    /// Returns all input meter data as (device_id, levels) pairs.
    #[must_use]
    pub fn all_input_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
        let contexts = self.input.read();
        contexts
            .iter()
            .map(|(device_id, ctx)| {
                let levels = ctx.meters().all_levels();
                ctx.meters().reset();
                (device_id.clone(), levels)
            })
            .collect()
    }

    /// Returns all output meter data as (device_id, levels) pairs.
    #[must_use]
    pub fn all_output_meters(&self) -> Vec<(String, Vec<MeterLevels>)> {
        let contexts = self.output.read();
        contexts
            .iter()
            .map(|(device_id, ctx)| {
                let levels = ctx.meters().all_levels();
                ctx.meters().reset();
                (device_id.clone(), levels)
            })
            .collect()
    }

    /// Clears all contexts.
    pub fn clear(&self) {
        self.input.write().clear();
        self.output.write().clear();
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
    /// Thread-safe metering contexts (separated for cross-thread access).
    metering_contexts: Arc<MeteringContexts>,
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
            metering_contexts: Arc::new(MeteringContexts::new()),
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

    /// Sets the VBAN manager for cross-node audio.
    ///
    /// This must be called after the VbanManager is created.
    pub fn set_vban_manager(&self, vban_manager: Arc<crate::vban_manager::VbanManager>) {
        self.route_manager.set_vban_manager(vban_manager);
    }

    /// Sets up the stream starter callback for RouteManager.
    ///
    /// This enables the RouteController to start input streams when needed
    /// for cross-node audio subscriptions.
    pub fn setup_stream_starter(self: &Arc<Self>) {
        let processor = Arc::downgrade(self);
        let starter: crate::route_manager::StreamStarterFn =
            Arc::new(move |device_id: &str, channels: &[u16]| {
                let processor = processor
                    .upgrade()
                    .ok_or_else(|| anyhow!("AudioProcessor dropped"))?;
                processor.ensure_input_stream_with_buffers(device_id, channels)
            });
        self.route_manager.set_stream_starter(starter);
    }

    /// Sets up the output stream starter callback for RouteManager.
    ///
    /// This enables the RouteController to start output streams when needed
    /// for cross-node audio subscriptions (receiver side).
    pub fn setup_output_stream_starter(self: &Arc<Self>) {
        let processor = Arc::downgrade(self);
        let starter: crate::route_manager::OutputStreamStarterFn =
            Arc::new(move |device_id: &str| {
                let processor = processor
                    .upgrade()
                    .ok_or_else(|| anyhow!("AudioProcessor dropped"))?;
                processor.start_output_stream(device_id)
            });
        self.route_manager.set_output_stream_starter(starter);
    }

    /// Sets up metering contexts access for RouteManager.
    ///
    /// This enables the RouteController to access meter levels for WebSocket broadcast.
    pub fn setup_metering_contexts(self: &Arc<Self>) {
        self.route_manager
            .set_metering_contexts(Arc::clone(&self.metering_contexts));
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

        // Clear metering contexts
        self.metering_contexts.clear();

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
        self.start_input_stream_with_config(device_id, None)
            .map(|_| ())
    }

    /// Starts an input stream with optional sample rate override.
    /// Returns the actual sample rate used.
    ///
    /// Note: For ASIO devices, the sample rate must be set by fully releasing
    /// the device first (call `release_device_for_reconfigure`), then creating
    /// a new stream with the desired rate.
    pub fn start_input_stream_with_config(
        &self,
        device_id: &str,
        sample_rate: Option<u32>,
    ) -> Result<u32> {
        info!("start_input_stream called for device: {device_id}");

        if self.input_streams.read().contains_key(device_id) {
            return Err(anyhow!(
                "Input stream already exists for device: {device_id}"
            ));
        }

        let (device, extended_config) = find_cpal_device(device_id, DeviceDirection::Input)
            .map_err(|e| {
                warn!("find_cpal_device failed for {device_id}: {e}");
                e
            })?;
        let default_rate = extended_config.config.sample_rate.0;
        let mut config = extended_config.config;
        let sample_format = extended_config.sample_format;
        let channels = config.channels;

        // Override sample rate if specified
        let requested_rate = sample_rate.unwrap_or(default_rate);
        config.sample_rate = cpal::SampleRate(requested_rate);
        info!(
            "Device {device_id}: requesting sample rate {}Hz (device default: {}Hz)",
            requested_rate, default_rate
        );

        info!("Device {device_id}: sample format {:?}", sample_format);

        let (context, buffer_indices) = self.create_input_context(device_id, channels as usize)?;

        info!(
            "Building input stream on {}: {} channels @ {}Hz, format {:?}, buffers {:?}",
            device_id, channels, requested_rate, sample_format, buffer_indices
        );

        let stream = build_input_stream(&device, &config, sample_format, Arc::clone(&context))
            .map_err(|e| {
                anyhow!(
                    "Failed to build input stream at {}Hz for {}: {}. \
                     Hint: For ASIO devices, ensure device is fully released before \
                     changing sample rate (use release_device_for_reconfigure).",
                    requested_rate,
                    device_id,
                    e
                )
            })?;
        stream
            .play()
            .map_err(|e| anyhow!("Failed to start input stream: {e}"))?;

        // Register context for metering access (thread-safe)
        self.metering_contexts
            .register_input(device_id, Arc::clone(&context));

        // Register with stream registry for API visibility
        let stream_id = format!("input-{device_id}");
        let active_stream = Arc::new(ActiveInputStream::new(
            stream_id,
            device_id,
            StreamConfig {
                sample_rate: requested_rate,
                buffer_size: 256, // Default buffer size
                channels,
            },
            buffer_indices.clone(),
            context,
        ));
        active_stream.set_running(true);
        self.stream_registry().register_input(active_stream);
        info!(
            "Registered input stream in registry, count: {}",
            self.stream_registry().input_count()
        );

        self.input_streams.write().insert(
            device_id.to_string(),
            CpalStreamHandle {
                stream: SyncStream(stream),
                device_id: device_id.to_string(),
                channels,
                sample_rate: requested_rate,
            },
        );

        info!("Input stream started on {device_id} at {requested_rate}Hz");
        Ok(requested_rate)
    }

    /// Ensures an input stream is running and returns buffer indices for the requested channels.
    ///
    /// If the stream is already running, returns the existing buffer indices.
    /// If not, starts the stream and returns the new buffer indices.
    pub fn ensure_input_stream_with_buffers(
        &self,
        device_id: &str,
        channels: &[u16],
    ) -> Result<Vec<usize>> {
        info!(
            "ensure_input_stream_with_buffers: device='{}', channels={:?}",
            device_id, channels
        );

        // Check if stream is already running
        if let Some(indices) = self.metering_contexts.get_input_buffer_indices(device_id) {
            info!(
                "Input stream already running for '{}', existing buffer indices: {:?}",
                device_id, indices
            );
            // Stream is running, return buffer indices for requested channels
            let requested: Vec<usize> = channels
                .iter()
                .filter_map(|&ch| {
                    let idx = (ch as usize).saturating_sub(1); // 1-based to 0-based
                    indices.get(idx).copied()
                })
                .collect();

            if requested.len() != channels.len() {
                return Err(anyhow!(
                    "Not all requested channels available on device {device_id}"
                ));
            }
            info!(
                "Returning existing buffer indices for requested channels: {:?}",
                requested
            );
            return Ok(requested);
        }

        // Stream not running, start it
        info!(
            "Input stream not running, starting stream for '{}'",
            device_id
        );
        self.start_input_stream(device_id)?;

        // Get the buffer indices from the newly started stream
        let indices = self
            .metering_contexts
            .get_input_buffer_indices(device_id)
            .ok_or_else(|| anyhow!("Failed to get buffer indices after starting stream"))?;

        info!("Stream started, buffer indices: {:?}", indices);

        let requested: Vec<usize> = channels
            .iter()
            .filter_map(|&ch| {
                let idx = (ch as usize).saturating_sub(1); // 1-based to 0-based
                indices.get(idx).copied()
            })
            .collect();

        if requested.len() != channels.len() {
            return Err(anyhow!(
                "Not all requested channels available on device {device_id}"
            ));
        }

        info!("Returning buffer indices for new stream: {:?}", requested);
        Ok(requested)
    }

    /// Starts an output stream for the specified device.
    pub fn start_output_stream(&self, device_id: &str) -> Result<()> {
        self.start_output_stream_with_config(device_id, None)
            .map(|_| ())
    }

    /// Starts an output stream with optional sample rate override.
    /// Returns the actual sample rate used.
    ///
    /// Note: For ASIO devices, the sample rate must be set by fully releasing
    /// the device first (call `release_device_for_reconfigure`), then creating
    /// a new stream with the desired rate.
    pub fn start_output_stream_with_config(
        &self,
        device_id: &str,
        sample_rate: Option<u32>,
    ) -> Result<u32> {
        info!("start_output_stream called for device: {device_id}");

        if self.output_streams.read().contains_key(device_id) {
            return Err(anyhow!(
                "Output stream already exists for device: {device_id}"
            ));
        }

        let (device, extended_config) = find_cpal_device(device_id, DeviceDirection::Output)
            .map_err(|e| {
                warn!("find_cpal_device failed for {device_id}: {e}");
                e
            })?;
        let default_rate = extended_config.config.sample_rate.0;
        let mut config = extended_config.config;
        let sample_format = extended_config.sample_format;
        let channels = config.channels;

        // Override sample rate if specified
        let requested_rate = sample_rate.unwrap_or(default_rate);
        config.sample_rate = cpal::SampleRate(requested_rate);
        info!(
            "Device {device_id}: requesting sample rate {}Hz (device default: {}Hz)",
            requested_rate, default_rate
        );

        info!("Device {device_id}: sample format {:?}", sample_format);

        let (context, dest_indices) = self.create_output_context(device_id, channels as usize);

        info!(
            "Building output stream on {}: {} channels @ {}Hz, format {:?}, destinations {:?}",
            device_id, channels, requested_rate, sample_format, dest_indices
        );

        let stream = build_output_stream(&device, &config, sample_format, Arc::clone(&context))
            .map_err(|e| {
                anyhow!(
                    "Failed to build output stream at {}Hz for {}: {}. \
                     Hint: For ASIO devices, ensure device is fully released before \
                     changing sample rate (use release_device_for_reconfigure).",
                    requested_rate,
                    device_id,
                    e
                )
            })?;
        stream
            .play()
            .map_err(|e| anyhow!("Failed to start output stream: {e}"))?;

        // Register context for metering access (thread-safe)
        self.metering_contexts
            .register_output(device_id, Arc::clone(&context));

        // Register with stream registry for API visibility
        let stream_id = format!("output-{device_id}");
        let active_stream = Arc::new(ActiveOutputStream::new(
            stream_id,
            device_id,
            StreamConfig {
                sample_rate: requested_rate,
                buffer_size: 256, // Default buffer size
                channels,
            },
            dest_indices.clone(),
            context,
        ));
        active_stream.set_running(true);
        self.stream_registry().register_output(active_stream);
        info!(
            "Registered output stream in registry, count: {}",
            self.stream_registry().output_count()
        );

        self.output_streams.write().insert(
            device_id.to_string(),
            CpalStreamHandle {
                stream: SyncStream(stream),
                device_id: device_id.to_string(),
                channels,
                sample_rate: requested_rate,
            },
        );

        info!("Output stream started on {device_id} at {requested_rate}Hz");
        Ok(requested_rate)
    }

    /// Stops an input stream for the specified device.
    pub fn stop_input_stream(&self, device_id: &str) -> Result<()> {
        let removed = self.input_streams.write().remove(device_id);
        if removed.is_some() {
            self.metering_contexts.unregister_input(device_id);
            self.stream_registry().unregister_input(device_id);
            info!("Input stream stopped on {device_id}");
            Ok(())
        } else {
            Err(anyhow!("No input stream found for device: {device_id}"))
        }
    }

    /// Stops an output stream for the specified device.
    pub fn stop_output_stream(&self, device_id: &str) -> Result<()> {
        self.metering_contexts.unregister_output(device_id);
        self.stream_registry().unregister_output(device_id);
        let removed = self.output_streams.write().remove(device_id);
        if removed.is_some() {
            info!("Output stream stopped on {device_id}");
            Ok(())
        } else {
            Err(anyhow!("No output stream found for device: {device_id}"))
        }
    }

    /// Stops all streams (input and output) for the specified device.
    ///
    /// This is used when a device is detached to clean up all associated streams.
    /// Errors are logged but not propagated since a device may only have one stream type.
    pub fn stop_device_streams(&self, device_id: &str) {
        // Try to stop input stream (may not exist)
        if self.stop_input_stream(device_id).is_ok() {
            info!("Stopped input stream for detached device: {device_id}");
        }

        // Try to stop output stream (may not exist)
        if self.stop_output_stream(device_id).is_ok() {
            info!("Stopped output stream for detached device: {device_id}");
        }
    }

    /// Releases a device completely to allow sample rate reconfiguration.
    ///
    /// For ASIO devices, the driver locks the sample rate while streams are active.
    /// This method:
    /// 1. Stops all streams for the device
    /// 2. Drops the cpal device handles (releases ASIO driver)
    /// 3. Waits briefly for the driver to fully release
    ///
    /// After calling this, you can create new streams at a different sample rate.
    /// The device enumeration will return fresh configuration from the ASIO driver.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The device ID to release
    /// * `wait_ms` - Milliseconds to wait for driver release (default: 100)
    pub fn release_device_for_reconfigure(&self, device_id: &str, wait_ms: u64) {
        info!(
            "Releasing device {} for reconfiguration (wait {}ms)",
            device_id, wait_ms
        );

        // Stop all streams - this drops the stream handles
        self.stop_device_streams(device_id);

        // Brief wait for ASIO driver to fully release
        // This is necessary because ASIO drivers may hold resources briefly after stream stop
        if wait_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        }

        info!("Device {} released, ready for reconfiguration", device_id);
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

    /// Returns the metering contexts for thread-safe metering access.
    ///
    /// This returns an Arc that can be cloned and shared across threads
    /// for periodic metering broadcasts.
    #[must_use]
    pub fn metering_contexts(&self) -> &Arc<MeteringContexts> {
        &self.metering_contexts
    }

    /// Returns a list of all active input device IDs.
    #[must_use]
    pub fn active_input_devices(&self) -> Vec<String> {
        self.input_streams.read().keys().cloned().collect()
    }

    /// Returns a list of all active output device IDs.
    #[must_use]
    pub fn active_output_devices(&self) -> Vec<String> {
        self.output_streams.read().keys().cloned().collect()
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
#[path = "audio_processor_tests.rs"]
mod tests;
