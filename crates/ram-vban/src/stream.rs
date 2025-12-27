//! VBAN stream lifecycle management.
//!
//! This module provides stream state tracking, timeout detection,
//! and coordinated send/receive operations for VBAN audio streams.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::jitter::{JitterBuffer, JitterBufferConfig, JitterBufferState};
use crate::protocol::VbanSampleRate;

/// Stream state for lifecycle management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Stream is starting up.
    Starting,
    /// Stream is online and receiving data.
    Online,
    /// Stream has gone offline (timeout).
    Offline,
    /// Stream has been stopped.
    Stopped,
}

/// Information about a managed VBAN stream.
#[derive(Debug)]
pub struct StreamInfo {
    /// Stream name.
    pub name: String,
    /// Remote address.
    pub remote_addr: SocketAddr,
    /// Sample rate.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u8,
    /// Current state.
    pub state: StreamState,
    /// Last packet timestamp.
    pub last_packet: Option<Instant>,
}

/// A managed receive stream with lifecycle tracking.
pub struct ManagedReceiveStream {
    /// Stream name.
    name: String,
    /// Remote source address.
    source: SocketAddr,
    /// Sample rate code.
    sample_rate: VbanSampleRate,
    /// Number of channels.
    channels: u8,
    /// Current state.
    state: RwLock<StreamState>,
    /// Jitter buffer for audio.
    jitter_buffer: RwLock<JitterBuffer>,
    /// Last packet timestamp.
    last_packet: RwLock<Option<Instant>>,
    /// Timeout duration.
    timeout: Duration,
    /// Whether the stream is running.
    running: AtomicBool,
    /// Total bytes received.
    bytes_received: AtomicU64,
}

impl ManagedReceiveStream {
    /// Creates a new managed receive stream.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        source: SocketAddr,
        sample_rate: VbanSampleRate,
        channels: u8,
        jitter_config: JitterBufferConfig,
    ) -> Self {
        let timeout_ms = jitter_config.timeout_ms;
        Self {
            name: name.into(),
            source,
            sample_rate,
            channels,
            state: RwLock::new(StreamState::Starting),
            jitter_buffer: RwLock::new(JitterBuffer::new(jitter_config)),
            last_packet: RwLock::new(None),
            timeout: Duration::from_millis(u64::from(timeout_ms)),
            running: AtomicBool::new(true),
            bytes_received: AtomicU64::new(0),
        }
    }

    /// Returns the stream name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the source address.
    #[must_use]
    pub fn source(&self) -> SocketAddr {
        self.source
    }

    /// Returns the sample rate in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.to_hz()
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Returns the current state.
    #[must_use]
    pub fn state(&self) -> StreamState {
        *self.state.read()
    }

    /// Returns total bytes received.
    #[must_use]
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Returns stream information.
    #[must_use]
    pub fn info(&self) -> StreamInfo {
        StreamInfo {
            name: self.name.clone(),
            remote_addr: self.source,
            sample_rate: self.sample_rate.to_hz(),
            channels: self.channels,
            state: self.state(),
            last_packet: *self.last_packet.read(),
        }
    }

    /// Pushes received audio data into the stream.
    pub fn push_audio(&self, sequence: u32, samples: Vec<f32>) {
        let now = Instant::now();
        *self.last_packet.write() = Some(now);

        let mut state = self.state.write();
        if *state == StreamState::Starting || *state == StreamState::Offline {
            debug!("Stream '{}' is now online", self.name);
            *state = StreamState::Online;
        }
        drop(state);

        self.bytes_received
            .fetch_add((samples.len() * 4) as u64, Ordering::Relaxed);
        self.jitter_buffer.write().push(sequence, samples);
    }

    /// Reads audio samples from the stream.
    ///
    /// Returns the number of samples actually read.
    pub fn read_audio(&self, output: &mut [f32]) -> usize {
        self.jitter_buffer.write().read(output)
    }

    /// Checks for timeout and updates state if needed.
    ///
    /// Returns true if the stream is still online.
    pub fn check_timeout(&self) -> bool {
        let last = *self.last_packet.read();

        if let Some(last_time) = last {
            if last_time.elapsed() > self.timeout {
                let mut state = self.state.write();
                if *state == StreamState::Online {
                    warn!("Stream '{}' timed out", self.name);
                    *state = StreamState::Offline;
                    self.jitter_buffer.write().set_offline();
                }
                return false;
            }
        }

        true
    }

    /// Stops the stream.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        *self.state.write() = StreamState::Stopped;
    }

    /// Returns whether the stream is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Resets the stream state for reconnection.
    pub fn reset(&self) {
        *self.state.write() = StreamState::Starting;
        *self.last_packet.write() = None;
        self.jitter_buffer.write().reset();
    }

    /// Returns the current buffer level in milliseconds.
    #[must_use]
    pub fn buffer_ms(&self) -> f32 {
        self.jitter_buffer.read().buffer_ms()
    }

    /// Returns the current jitter buffer state.
    #[must_use]
    pub fn buffer_state(&self) -> JitterBufferState {
        self.jitter_buffer.read().state()
    }
}

/// Event types for stream lifecycle changes.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A new stream was discovered.
    Discovered { name: String, source: SocketAddr },
    /// Stream came online.
    Online { name: String },
    /// Stream went offline.
    Offline { name: String },
    /// Stream was removed.
    Removed { name: String },
}

/// Manager for multiple VBAN receive streams.
pub struct StreamManager {
    /// Active streams.
    streams: RwLock<Vec<Arc<ManagedReceiveStream>>>,
    /// Event sender.
    event_tx: Option<mpsc::UnboundedSender<StreamEvent>>,
    /// Default jitter buffer configuration.
    default_jitter_config: JitterBufferConfig,
}

impl StreamManager {
    /// Creates a new stream manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            streams: RwLock::new(Vec::new()),
            event_tx: None,
            default_jitter_config: JitterBufferConfig::default(),
        }
    }

    /// Creates a stream manager with event notifications.
    #[must_use]
    pub fn with_events(event_tx: mpsc::UnboundedSender<StreamEvent>) -> Self {
        Self {
            streams: RwLock::new(Vec::new()),
            event_tx: Some(event_tx),
            default_jitter_config: JitterBufferConfig::default(),
        }
    }

    /// Sets the default jitter buffer configuration.
    pub fn set_jitter_config(&mut self, config: JitterBufferConfig) {
        self.default_jitter_config = config;
    }

    /// Adds or updates a stream.
    ///
    /// If the stream already exists, it will be updated with new audio.
    /// If not, a new stream will be created.
    #[allow(clippy::too_many_arguments)]
    pub fn add_or_update_stream(
        &self,
        name: &str,
        source: SocketAddr,
        sample_rate: VbanSampleRate,
        channels: u8,
        sequence: u32,
        samples: Vec<f32>,
    ) -> Arc<ManagedReceiveStream> {
        // Try to find existing stream
        {
            let streams = self.streams.read();
            if let Some(stream) = streams.iter().find(|s| s.name() == name) {
                stream.push_audio(sequence, samples);
                return Arc::clone(stream);
            }
        }

        // Create new stream
        let config = JitterBufferConfig {
            sample_rate: sample_rate.to_hz(),
            channels: channels as usize,
            ..self.default_jitter_config.clone()
        };

        let stream = Arc::new(ManagedReceiveStream::new(
            name,
            source,
            sample_rate,
            channels,
            config,
        ));

        stream.push_audio(sequence, samples);

        self.streams.write().push(Arc::clone(&stream));

        self.send_event(StreamEvent::Discovered {
            name: name.to_string(),
            source,
        });

        stream
    }

    /// Gets a stream by name.
    #[must_use]
    pub fn get_stream(&self, name: &str) -> Option<Arc<ManagedReceiveStream>> {
        self.streams
            .read()
            .iter()
            .find(|s| s.name() == name)
            .cloned()
    }

    /// Removes a stream by name.
    pub fn remove_stream(&self, name: &str) -> bool {
        let mut streams = self.streams.write();
        let initial_len = streams.len();
        streams.retain(|s| s.name() != name);

        if streams.len() == initial_len {
            false
        } else {
            self.send_event(StreamEvent::Removed {
                name: name.to_string(),
            });
            true
        }
    }

    /// Returns all active streams.
    #[must_use]
    pub fn streams(&self) -> Vec<Arc<ManagedReceiveStream>> {
        self.streams.read().clone()
    }

    /// Returns information about all streams.
    #[must_use]
    pub fn stream_infos(&self) -> Vec<StreamInfo> {
        self.streams.read().iter().map(|s| s.info()).collect()
    }

    /// Checks all streams for timeout.
    ///
    /// Returns the names of streams that went offline.
    pub fn check_timeouts(&self) -> Vec<String> {
        let mut offline = Vec::new();

        for stream in self.streams.read().iter() {
            let was_online = stream.state() == StreamState::Online;
            if !stream.check_timeout() && was_online {
                offline.push(stream.name().to_string());
                self.send_event(StreamEvent::Offline {
                    name: stream.name().to_string(),
                });
            }
        }

        offline
    }

    /// Returns the number of active streams.
    #[must_use]
    pub fn stream_count(&self) -> usize {
        self.streams.read().len()
    }

    fn send_event(&self, event: StreamEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_jitter_config() -> JitterBufferConfig {
        JitterBufferConfig {
            sample_rate: 48000,
            channels: 2,
            min_buffer_ms: 1.0,
            max_buffer_ms: 5.0,
            initial_buffer_ms: 2.0,
            timeout_ms: 100,
        }
    }

    #[test]
    fn managed_stream_new() {
        let stream = ManagedReceiveStream::new(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            test_jitter_config(),
        );

        assert_eq!(stream.name(), "test");
        assert_eq!(stream.state(), StreamState::Starting);
        assert_eq!(stream.sample_rate(), 48000);
        assert_eq!(stream.channels(), 2);
    }

    #[test]
    fn managed_stream_push_audio() {
        let stream = ManagedReceiveStream::new(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            test_jitter_config(),
        );

        stream.push_audio(0, vec![0.5; 256]);

        assert_eq!(stream.state(), StreamState::Online);
        assert!(stream.bytes_received() > 0);
    }

    #[test]
    fn managed_stream_read_audio() {
        let stream = ManagedReceiveStream::new(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            JitterBufferConfig {
                initial_buffer_ms: 0.5, // Lower for testing
                ..test_jitter_config()
            },
        );

        // Push enough data
        for i in 0..4 {
            stream.push_audio(i, vec![0.5; 64]);
        }

        let mut output = [0.0; 64];
        let read = stream.read_audio(&mut output);
        assert!(read > 0);
    }

    #[test]
    fn managed_stream_stop() {
        let stream = ManagedReceiveStream::new(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            test_jitter_config(),
        );

        assert!(stream.is_running());
        stream.stop();
        assert!(!stream.is_running());
        assert_eq!(stream.state(), StreamState::Stopped);
    }

    #[test]
    fn managed_stream_info() {
        let stream = ManagedReceiveStream::new(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            test_jitter_config(),
        );

        let info = stream.info();
        assert_eq!(info.name, "test");
        assert_eq!(info.sample_rate, 48000);
        assert_eq!(info.channels, 2);
    }

    #[test]
    fn stream_manager_new() {
        let manager = StreamManager::new();
        assert_eq!(manager.stream_count(), 0);
    }

    #[test]
    fn stream_manager_add_stream() {
        let manager = StreamManager::new();

        let stream = manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        assert_eq!(stream.name(), "test");
        assert_eq!(manager.stream_count(), 1);
    }

    #[test]
    fn stream_manager_update_existing() {
        let manager = StreamManager::new();

        manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            1,
            vec![0.6; 256],
        );

        // Should still be 1 stream
        assert_eq!(manager.stream_count(), 1);

        let stream = manager.get_stream("test").unwrap();
        assert!(stream.bytes_received() > 256 * 4);
    }

    #[test]
    fn stream_manager_get_stream() {
        let manager = StreamManager::new();

        manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        assert!(manager.get_stream("test").is_some());
        assert!(manager.get_stream("nonexistent").is_none());
    }

    #[test]
    fn stream_manager_remove_stream() {
        let manager = StreamManager::new();

        manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        assert!(manager.remove_stream("test"));
        assert!(!manager.remove_stream("test")); // Already removed
        assert_eq!(manager.stream_count(), 0);
    }

    #[test]
    fn stream_manager_stream_infos() {
        let manager = StreamManager::new();

        manager.add_or_update_stream(
            "stream1",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        manager.add_or_update_stream(
            "stream2",
            "127.0.0.1:6981".parse().unwrap(),
            VbanSampleRate::Hz44100,
            1,
            0,
            vec![0.5; 256],
        );

        let infos = manager.stream_infos();
        assert_eq!(infos.len(), 2);
    }

    #[tokio::test]
    async fn stream_manager_with_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let manager = StreamManager::with_events(tx);

        manager.add_or_update_stream(
            "test",
            "127.0.0.1:6980".parse().unwrap(),
            VbanSampleRate::Hz48000,
            2,
            0,
            vec![0.5; 256],
        );

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::Discovered { name, .. } => assert_eq!(name, "test"),
            _ => panic!("Expected Discovered event"),
        }
    }
}
