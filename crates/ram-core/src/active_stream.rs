//! Active audio stream types for managing running input/output streams.
//!
//! This module defines the types that wrap cpal streams with their associated
//! routing configuration. Active streams are created when audio processing
//! starts and destroyed when it stops.

use crate::callbacks::{InputCallbackContext, OutputCallbackContext};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Configuration for an active audio stream.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Buffer size in samples per channel.
    pub buffer_size: u32,
    /// Number of channels.
    pub channels: u16,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 256,
            channels: 2,
        }
    }
}

/// State of an active stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamState {
    /// Stream is created but not started.
    Stopped = 0,
    /// Stream is actively processing audio.
    Running = 1,
    /// Stream encountered an error.
    Error = 2,
}

impl StreamState {
    /// Converts from u8.
    #[must_use]
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Stopped,
            1 => Self::Running,
            _ => Self::Error,
        }
    }
}

/// Statistics for an active stream.
#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)] // Field names are semantic: callback_count, underrun_count, overrun_count
pub struct StreamStats {
    /// Number of callbacks processed.
    pub callback_count: AtomicU64,
    /// Number of buffer underruns (output couldn't get enough samples).
    pub underrun_count: AtomicU64,
    /// Number of buffer overruns (input couldn't write samples fast enough).
    pub overrun_count: AtomicU64,
}

impl StreamStats {
    /// Creates new stream statistics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the callback count.
    pub fn record_callback(&self) {
        self.callback_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the underrun count.
    pub fn record_underrun(&self) {
        self.underrun_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments the overrun count.
    pub fn record_overrun(&self) {
        self.overrun_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Gets the callback count.
    #[must_use]
    pub fn callbacks(&self) -> u64 {
        self.callback_count.load(Ordering::Relaxed)
    }

    /// Gets the underrun count.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    /// Gets the overrun count.
    #[must_use]
    pub fn overruns(&self) -> u64 {
        self.overrun_count.load(Ordering::Relaxed)
    }
}

impl Clone for StreamStats {
    fn clone(&self) -> Self {
        Self {
            callback_count: AtomicU64::new(self.callbacks()),
            underrun_count: AtomicU64::new(self.underruns()),
            overrun_count: AtomicU64::new(self.overruns()),
        }
    }
}

/// An active input stream that captures audio from a device.
///
/// This wraps the cpal stream with its callback context and configuration.
/// The stream writes captured audio to ring buffers in the buffer pool.
#[derive(Debug)]
pub struct ActiveInputStream {
    /// Unique identifier for this stream.
    id: String,
    /// Device ID this stream is capturing from.
    device_id: String,
    /// Stream configuration.
    config: StreamConfig,
    /// Buffer indices for each channel (into the ring buffer pool).
    buffer_indices: Vec<usize>,
    /// Callback context (shared with the audio callback).
    context: Arc<InputCallbackContext>,
    /// Whether the stream is running.
    running: AtomicBool,
    /// Stream statistics.
    stats: StreamStats,
}

impl ActiveInputStream {
    /// Creates a new active input stream.
    ///
    /// Note: This only creates the stream metadata. The actual cpal stream
    /// must be created separately using the callback from `callbacks::create_input_callback`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique stream identifier
    /// * `device_id` - Device ID being captured from
    /// * `config` - Stream configuration
    /// * `buffer_indices` - Ring buffer indices for each channel
    /// * `context` - Callback context for audio processing
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        device_id: impl Into<String>,
        config: StreamConfig,
        buffer_indices: Vec<usize>,
        context: Arc<InputCallbackContext>,
    ) -> Self {
        Self {
            id: id.into(),
            device_id: device_id.into(),
            config,
            buffer_indices,
            context,
            running: AtomicBool::new(false),
            stats: StreamStats::new(),
        }
    }

    /// Returns the stream ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the device ID.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the stream configuration.
    #[must_use]
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Returns the buffer indices for each channel.
    #[must_use]
    pub fn buffer_indices(&self) -> &[usize] {
        &self.buffer_indices
    }

    /// Returns the callback context.
    #[must_use]
    pub fn context(&self) -> &Arc<InputCallbackContext> {
        &self.context
    }

    /// Returns whether the stream is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Marks the stream as running.
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    /// Returns the stream statistics.
    #[must_use]
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.buffer_indices.len()
    }
}

/// An active output stream that plays audio to a device.
///
/// This wraps the cpal stream with its callback context and configuration.
/// The stream reads audio from the routing snapshot and buffer pool.
#[derive(Debug)]
pub struct ActiveOutputStream {
    /// Unique identifier for this stream.
    id: String,
    /// Device ID this stream is playing to.
    device_id: String,
    /// Stream configuration.
    config: StreamConfig,
    /// Destination indices in the routing snapshot.
    dest_indices: Vec<usize>,
    /// Callback context (shared with the audio callback).
    context: Arc<OutputCallbackContext>,
    /// Whether the stream is running.
    running: AtomicBool,
    /// Stream statistics.
    stats: StreamStats,
}

impl ActiveOutputStream {
    /// Creates a new active output stream.
    ///
    /// Note: This only creates the stream metadata. The actual cpal stream
    /// must be created separately using the callback from `callbacks::create_output_callback`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique stream identifier
    /// * `device_id` - Device ID being played to
    /// * `config` - Stream configuration
    /// * `dest_indices` - Destination indices in the routing snapshot
    /// * `context` - Callback context for audio processing
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        device_id: impl Into<String>,
        config: StreamConfig,
        dest_indices: Vec<usize>,
        context: Arc<OutputCallbackContext>,
    ) -> Self {
        Self {
            id: id.into(),
            device_id: device_id.into(),
            config,
            dest_indices,
            context,
            running: AtomicBool::new(false),
            stats: StreamStats::new(),
        }
    }

    /// Returns the stream ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the device ID.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the stream configuration.
    #[must_use]
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Returns the destination indices.
    #[must_use]
    pub fn dest_indices(&self) -> &[usize] {
        &self.dest_indices
    }

    /// Returns the callback context.
    #[must_use]
    pub fn context(&self) -> &Arc<OutputCallbackContext> {
        &self.context
    }

    /// Returns whether the stream is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Marks the stream as running.
    pub fn set_running(&self, running: bool) {
        self.running.store(running, Ordering::Release);
    }

    /// Returns the stream statistics.
    #[must_use]
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }

    /// Returns the number of output channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.dest_indices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring_buffer_pool::RingBufferPool;
    use crate::routing_table::RoutingTable;

    #[test]
    fn stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.buffer_size, 256);
        assert_eq!(config.channels, 2);
    }

    #[test]
    fn stream_state_from_u8() {
        assert_eq!(StreamState::from_u8(0), StreamState::Stopped);
        assert_eq!(StreamState::from_u8(1), StreamState::Running);
        assert_eq!(StreamState::from_u8(2), StreamState::Error);
        assert_eq!(StreamState::from_u8(99), StreamState::Error);
    }

    #[test]
    fn stream_stats_counting() {
        let stats = StreamStats::new();

        assert_eq!(stats.callbacks(), 0);
        assert_eq!(stats.underruns(), 0);
        assert_eq!(stats.overruns(), 0);

        stats.record_callback();
        stats.record_callback();
        assert_eq!(stats.callbacks(), 2);

        stats.record_underrun();
        assert_eq!(stats.underruns(), 1);

        stats.record_overrun();
        stats.record_overrun();
        stats.record_overrun();
        assert_eq!(stats.overruns(), 3);
    }

    #[test]
    fn active_input_stream_creation() {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let idx0 = pool.allocate().unwrap();
        let idx1 = pool.allocate().unwrap();

        let context = Arc::new(InputCallbackContext::new(
            Arc::clone(&pool),
            vec![idx0, idx1],
            "device-1",
        ));

        let stream = ActiveInputStream::new(
            "stream-1",
            "device-1",
            StreamConfig::default(),
            vec![idx0, idx1],
            context,
        );

        assert_eq!(stream.id(), "stream-1");
        assert_eq!(stream.device_id(), "device-1");
        assert_eq!(stream.channel_count(), 2);
        assert!(!stream.is_running());
    }

    #[test]
    fn active_input_stream_running_state() {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let context = Arc::new(InputCallbackContext::new(
            Arc::clone(&pool),
            vec![],
            "device",
        ));

        let stream =
            ActiveInputStream::new("stream", "device", StreamConfig::default(), vec![], context);

        assert!(!stream.is_running());
        stream.set_running(true);
        assert!(stream.is_running());
        stream.set_running(false);
        assert!(!stream.is_running());
    }

    #[test]
    fn active_output_stream_creation() {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let routing = Arc::new(RoutingTable::new());

        let context = Arc::new(OutputCallbackContext::new(
            Arc::clone(&routing),
            Arc::clone(&pool),
            vec![0, 1],
            "device-2",
        ));

        let stream = ActiveOutputStream::new(
            "stream-2",
            "device-2",
            StreamConfig::default(),
            vec![0, 1],
            context,
        );

        assert_eq!(stream.id(), "stream-2");
        assert_eq!(stream.device_id(), "device-2");
        assert_eq!(stream.channel_count(), 2);
        assert!(!stream.is_running());
    }

    #[test]
    fn active_output_stream_running_state() {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let routing = Arc::new(RoutingTable::new());
        let context = Arc::new(OutputCallbackContext::new(routing, pool, vec![], "device"));

        let stream =
            ActiveOutputStream::new("stream", "device", StreamConfig::default(), vec![], context);

        assert!(!stream.is_running());
        stream.set_running(true);
        assert!(stream.is_running());
    }
}
