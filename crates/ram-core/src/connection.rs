//! Connection state machine for audio streams.
//!
//! This module provides:
//! - Connection identifiers for audio routes
//! - Source connections with gain/mute control
//! - State machine for connection lifecycle
//! - Retry logic and health monitoring

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::atomic::AtomicF32;
use crate::buffer::RingBuffer;
use crate::Sample;

/// Connection states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ConnectionState {
    /// Connection not initialized.
    Disconnected = 0,
    /// Attempting to connect.
    Connecting = 1,
    /// Connection established and active.
    Connected = 2,
    /// Connection lost, attempting to reconnect.
    Reconnecting = 3,
    /// Connection failed, waiting before retry.
    Backoff = 4,
    /// Connection permanently failed.
    Failed = 5,
}

impl From<u8> for ConnectionState {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Reconnecting,
            4 => Self::Backoff,
            5 => Self::Failed,
            _ => Self::Disconnected,
        }
    }
}

/// Atomic wrapper for connection state.
pub struct AtomicConnectionState(AtomicU8);

impl AtomicConnectionState {
    /// Creates a new atomic state.
    #[must_use]
    pub const fn new(state: ConnectionState) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    /// Loads the current state.
    #[must_use]
    pub fn load(&self) -> ConnectionState {
        ConnectionState::from(self.0.load(Ordering::Acquire))
    }

    /// Stores a new state.
    pub fn store(&self, state: ConnectionState) {
        self.0.store(state as u8, Ordering::Release);
    }

    /// Attempts to transition from expected to new state.
    ///
    /// Returns true if the transition was successful.
    pub fn compare_exchange(&self, expected: ConnectionState, new: ConnectionState) -> bool {
        self.0
            .compare_exchange(
                expected as u8,
                new as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

impl Default for AtomicConnectionState {
    fn default() -> Self {
        Self::new(ConnectionState::Disconnected)
    }
}

/// Configuration for connection retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Initial backoff duration.
    pub initial_backoff: Duration,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// Backoff multiplier (e.g., 2.0 for exponential).
    pub backoff_multiplier: f64,
    /// Maximum number of retry attempts (0 = infinite).
    pub max_retries: u32,
    /// Connection timeout.
    pub connect_timeout: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            max_retries: 0, // Infinite
            connect_timeout: Duration::from_secs(10),
        }
    }
}

/// Connection health metrics.
#[derive(Debug, Clone, Default)]
pub struct ConnectionHealth {
    /// Number of successful connections.
    pub connect_count: u64,
    /// Number of failed connection attempts.
    pub failure_count: u64,
    /// Number of disconnections.
    pub disconnect_count: u64,
    /// Last successful connection time.
    pub last_connected: Option<Instant>,
    /// Last failure time.
    pub last_failure: Option<Instant>,
    /// Current uptime since last connection.
    pub uptime: Option<Duration>,
}

impl ConnectionHealth {
    /// Updates uptime based on last connection time.
    pub fn update_uptime(&mut self) {
        if let Some(connected_at) = self.last_connected {
            self.uptime = Some(connected_at.elapsed());
        }
    }

    /// Records a successful connection.
    pub fn record_connected(&mut self) {
        self.connect_count += 1;
        self.last_connected = Some(Instant::now());
    }

    /// Records a connection failure.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());
    }

    /// Records a disconnection.
    pub fn record_disconnected(&mut self) {
        self.disconnect_count += 1;
        self.uptime = None;
    }
}

/// Manages the connection state machine.
pub struct ConnectionManager {
    /// Current connection state.
    state: AtomicConnectionState,
    /// Retry configuration.
    config: RetryConfig,
    /// Current retry count.
    retry_count: AtomicU8,
    /// Current backoff duration.
    current_backoff: RwLock<Duration>,
    /// Health metrics.
    health: RwLock<ConnectionHealth>,
    /// Time of last state change.
    last_state_change: RwLock<Instant>,
}

impl ConnectionManager {
    /// Creates a new connection manager.
    #[must_use]
    pub fn new(config: RetryConfig) -> Self {
        Self {
            state: AtomicConnectionState::new(ConnectionState::Disconnected),
            config,
            retry_count: AtomicU8::new(0),
            current_backoff: RwLock::new(Duration::ZERO),
            health: RwLock::new(ConnectionHealth::default()),
            last_state_change: RwLock::new(Instant::now()),
        }
    }

    /// Creates a manager with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Returns the current connection state.
    #[must_use]
    pub fn state(&self) -> ConnectionState {
        self.state.load()
    }

    /// Returns the current retry count.
    #[must_use]
    pub fn retry_count(&self) -> u8 {
        self.retry_count.load(Ordering::Acquire)
    }

    /// Returns the current backoff duration.
    #[must_use]
    pub fn current_backoff(&self) -> Duration {
        *self.current_backoff.read()
    }

    /// Returns the health metrics.
    #[must_use]
    pub fn health(&self) -> ConnectionHealth {
        let mut health = self.health.read().clone();
        if self.state() == ConnectionState::Connected {
            health.update_uptime();
        }
        health
    }

    /// Returns the time since last state change.
    #[must_use]
    pub fn time_in_state(&self) -> Duration {
        self.last_state_change.read().elapsed()
    }

    /// Initiates a connection attempt.
    ///
    /// Returns true if transition was successful.
    pub fn connect(&self) -> bool {
        let current = self.state();
        if (current == ConnectionState::Disconnected || current == ConnectionState::Backoff)
            && self
                .state
                .compare_exchange(current, ConnectionState::Connecting)
        {
            *self.last_state_change.write() = Instant::now();
            return true;
        }
        false
    }

    /// Marks the connection as established.
    ///
    /// Returns true if transition was successful.
    pub fn connected(&self) -> bool {
        let current = self.state();
        if (current == ConnectionState::Connecting || current == ConnectionState::Reconnecting)
            && self
                .state
                .compare_exchange(current, ConnectionState::Connected)
        {
            self.retry_count.store(0, Ordering::Release);
            *self.current_backoff.write() = Duration::ZERO;
            self.health.write().record_connected();
            *self.last_state_change.write() = Instant::now();
            return true;
        }
        false
    }

    /// Marks the connection as lost, initiating reconnection.
    ///
    /// Returns true if transition was successful.
    pub fn disconnected(&self) -> bool {
        let current = self.state();
        if current == ConnectionState::Connected
            && self
                .state
                .compare_exchange(current, ConnectionState::Reconnecting)
        {
            self.health.write().record_disconnected();
            *self.last_state_change.write() = Instant::now();
            return true;
        }
        false
    }

    /// Records a connection failure and enters backoff.
    ///
    /// Returns the backoff duration, or None if max retries exceeded.
    pub fn failed(&self) -> Option<Duration> {
        let current = self.state();
        if current != ConnectionState::Connecting && current != ConnectionState::Reconnecting {
            return None;
        }

        let retry = self.retry_count.fetch_add(1, Ordering::AcqRel) + 1;

        // Check max retries
        if self.config.max_retries > 0 && u32::from(retry) >= self.config.max_retries {
            self.state.store(ConnectionState::Failed);
            self.health.write().record_failure();
            *self.last_state_change.write() = Instant::now();
            return None;
        }

        // Calculate backoff
        let backoff = {
            let mut backoff = *self.current_backoff.read();
            if backoff == Duration::ZERO {
                backoff = self.config.initial_backoff;
            } else {
                backoff =
                    Duration::from_secs_f64(backoff.as_secs_f64() * self.config.backoff_multiplier);
            }
            backoff = backoff.min(self.config.max_backoff);
            *self.current_backoff.write() = backoff;
            backoff
        };

        self.state.store(ConnectionState::Backoff);
        self.health.write().record_failure();
        *self.last_state_change.write() = Instant::now();

        Some(backoff)
    }

    /// Resets the connection to disconnected state.
    pub fn reset(&self) {
        self.state.store(ConnectionState::Disconnected);
        self.retry_count.store(0, Ordering::Release);
        *self.current_backoff.write() = Duration::ZERO;
        *self.last_state_change.write() = Instant::now();
    }

    /// Returns true if the connection can attempt to connect.
    #[must_use]
    pub fn can_connect(&self) -> bool {
        let state = self.state();
        state == ConnectionState::Disconnected || state == ConnectionState::Backoff
    }

    /// Returns true if currently connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.state() == ConnectionState::Connected
    }

    /// Returns true if in a failed state.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.state() == ConnectionState::Failed
    }

    /// Checks if backoff period has elapsed.
    #[must_use]
    pub fn backoff_elapsed(&self) -> bool {
        if self.state() != ConnectionState::Backoff {
            return false;
        }
        self.time_in_state() >= self.current_backoff()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_from_u8() {
        assert_eq!(ConnectionState::from(0), ConnectionState::Disconnected);
        assert_eq!(ConnectionState::from(1), ConnectionState::Connecting);
        assert_eq!(ConnectionState::from(2), ConnectionState::Connected);
        assert_eq!(ConnectionState::from(3), ConnectionState::Reconnecting);
        assert_eq!(ConnectionState::from(4), ConnectionState::Backoff);
        assert_eq!(ConnectionState::from(5), ConnectionState::Failed);
        assert_eq!(ConnectionState::from(99), ConnectionState::Disconnected);
    }

    #[test]
    fn atomic_connection_state() {
        let state = AtomicConnectionState::new(ConnectionState::Disconnected);
        assert_eq!(state.load(), ConnectionState::Disconnected);

        state.store(ConnectionState::Connected);
        assert_eq!(state.load(), ConnectionState::Connected);

        assert!(state.compare_exchange(ConnectionState::Connected, ConnectionState::Disconnected));
        assert_eq!(state.load(), ConnectionState::Disconnected);

        assert!(!state.compare_exchange(ConnectionState::Connected, ConnectionState::Failed));
        assert_eq!(state.load(), ConnectionState::Disconnected);
    }

    #[test]
    fn retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.initial_backoff, Duration::from_millis(500));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn connection_health_tracking() {
        let mut health = ConnectionHealth::default();
        assert_eq!(health.connect_count, 0);
        assert_eq!(health.failure_count, 0);

        health.record_connected();
        assert_eq!(health.connect_count, 1);
        assert!(health.last_connected.is_some());

        health.record_failure();
        assert_eq!(health.failure_count, 1);
        assert!(health.last_failure.is_some());

        health.record_disconnected();
        assert_eq!(health.disconnect_count, 1);
        assert!(health.uptime.is_none());
    }

    #[test]
    fn connection_manager_basic_flow() {
        let manager = ConnectionManager::with_defaults();
        assert_eq!(manager.state(), ConnectionState::Disconnected);
        assert!(manager.can_connect());
        assert!(!manager.is_connected());

        // Connect flow
        assert!(manager.connect());
        assert_eq!(manager.state(), ConnectionState::Connecting);
        assert!(!manager.can_connect());

        assert!(manager.connected());
        assert_eq!(manager.state(), ConnectionState::Connected);
        assert!(manager.is_connected());

        // Disconnect flow
        assert!(manager.disconnected());
        assert_eq!(manager.state(), ConnectionState::Reconnecting);
        assert!(!manager.is_connected());
    }

    #[test]
    fn connection_manager_failure_backoff() {
        let config = RetryConfig {
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(1000),
            backoff_multiplier: 2.0,
            max_retries: 3,
            connect_timeout: Duration::from_secs(5),
        };
        let manager = ConnectionManager::new(config);

        // Start connection
        manager.connect();
        assert_eq!(manager.state(), ConnectionState::Connecting);

        // First failure
        let backoff1 = manager.failed().unwrap();
        assert_eq!(backoff1, Duration::from_millis(100));
        assert_eq!(manager.state(), ConnectionState::Backoff);
        assert_eq!(manager.retry_count(), 1);

        // Retry
        manager.connect();
        let backoff2 = manager.failed().unwrap();
        assert_eq!(backoff2, Duration::from_millis(200));
        assert_eq!(manager.retry_count(), 2);

        // Third retry - should fail permanently
        manager.connect();
        let backoff3 = manager.failed();
        assert!(backoff3.is_none());
        assert_eq!(manager.state(), ConnectionState::Failed);
        assert!(manager.is_failed());
    }

    #[test]
    fn connection_manager_reset() {
        let manager = ConnectionManager::with_defaults();

        manager.connect();
        manager.connected();
        assert_eq!(manager.state(), ConnectionState::Connected);

        manager.reset();
        assert_eq!(manager.state(), ConnectionState::Disconnected);
        assert_eq!(manager.retry_count(), 0);
        assert_eq!(manager.current_backoff(), Duration::ZERO);
    }

    #[test]
    fn connection_manager_health_metrics() {
        let manager = ConnectionManager::with_defaults();

        manager.connect();
        manager.connected();
        let health = manager.health();
        assert_eq!(health.connect_count, 1);

        manager.disconnected();
        manager.failed();
        let health = manager.health();
        assert_eq!(health.disconnect_count, 1);
        assert_eq!(health.failure_count, 1);
    }

    #[test]
    fn connection_manager_backoff_elapsed() {
        let config = RetryConfig {
            initial_backoff: Duration::from_millis(1),
            ..Default::default()
        };
        let manager = ConnectionManager::new(config);

        manager.connect();
        manager.failed();
        assert_eq!(manager.state(), ConnectionState::Backoff);

        // Wait a bit for backoff to elapse
        std::thread::sleep(Duration::from_millis(5));
        assert!(manager.backoff_elapsed());
    }
}

// =============================================================================
// Connection Identifier and Source Connection
// =============================================================================

/// Identifies a unique audio connection between a source and destination channel.
///
/// The format follows `{src_node}:{src_device}:{src_ch}>{dst_node}:{dst_device}:{dst_ch}`.
/// For local connections, the node is "LOCAL".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId {
    /// Source node name.
    pub source_node: String,
    /// Source device name.
    pub source_device: String,
    /// Source channel (1-based).
    pub source_channel: u16,
    /// Destination node name.
    pub destination_node: String,
    /// Destination device name.
    pub destination_device: String,
    /// Destination channel (1-based).
    pub destination_channel: u16,
}

impl ConnectionId {
    /// Creates a new connection ID.
    ///
    /// # Panics
    ///
    /// Panics if channel numbers are 0 (channels are 1-based).
    #[must_use]
    pub fn new(
        source_node: impl Into<String>,
        source_device: impl Into<String>,
        source_channel: u16,
        destination_node: impl Into<String>,
        destination_device: impl Into<String>,
        destination_channel: u16,
    ) -> Self {
        assert!(source_channel > 0, "source channel must be 1-based (got 0)");
        assert!(
            destination_channel > 0,
            "destination channel must be 1-based (got 0)"
        );
        Self {
            source_node: source_node.into(),
            source_device: source_device.into(),
            source_channel,
            destination_node: destination_node.into(),
            destination_device: destination_device.into(),
            destination_channel,
        }
    }

    /// Creates a new connection ID with validation, returning an error if invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Channel numbers are 0 (channels are 1-based)
    /// - Node or device names are empty
    pub fn try_new(
        source_node: impl Into<String>,
        source_device: impl Into<String>,
        source_channel: u16,
        destination_node: impl Into<String>,
        destination_device: impl Into<String>,
        destination_channel: u16,
    ) -> Result<Self, &'static str> {
        let source_node = source_node.into();
        let source_device = source_device.into();
        let destination_node = destination_node.into();
        let destination_device = destination_device.into();

        if source_channel == 0 {
            return Err("source channel must be 1-based (got 0)");
        }
        if destination_channel == 0 {
            return Err("destination channel must be 1-based (got 0)");
        }
        if source_node.is_empty() {
            return Err("source node name cannot be empty");
        }
        if source_device.is_empty() {
            return Err("source device name cannot be empty");
        }
        if destination_node.is_empty() {
            return Err("destination node name cannot be empty");
        }
        if destination_device.is_empty() {
            return Err("destination device name cannot be empty");
        }

        Ok(Self {
            source_node,
            source_device,
            source_channel,
            destination_node,
            destination_device,
            destination_channel,
        })
    }

    /// Creates a local connection ID (both source and destination on this node).
    #[must_use]
    pub fn local(
        source_device: impl Into<String>,
        source_channel: u16,
        destination_device: impl Into<String>,
        destination_channel: u16,
    ) -> Self {
        Self::new(
            "LOCAL",
            source_device,
            source_channel,
            "LOCAL",
            destination_device,
            destination_channel,
        )
    }

    /// Returns the destination ID string (for routing matrix lookup).
    #[must_use]
    pub fn destination_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.destination_node, self.destination_device, self.destination_channel
        )
    }

    /// Returns the source ID string.
    #[must_use]
    pub fn source_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.source_node, self.source_device, self.source_channel
        )
    }

    /// Returns true if this is a local-only connection.
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.source_node == "LOCAL" && self.destination_node == "LOCAL"
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}->{}:{}:{}",
            self.source_node,
            self.source_device,
            self.source_channel,
            self.destination_node,
            self.destination_device,
            self.destination_channel
        )
    }
}

/// A source connection that feeds audio into a destination channel.
///
/// This represents a single audio path from a source (input device, network stream, etc.)
/// to a destination channel. It includes:
/// - A ring buffer for audio data
/// - Gain and mute controls (atomic for real-time safety)
/// - Enable/disable state
pub struct SourceConnection {
    /// Connection identifier.
    id: ConnectionId,
    /// Audio buffer for incoming samples.
    buffer: RingBuffer,
    /// Gain multiplier (1.0 = unity).
    gain: AtomicF32,
    /// Mute state.
    muted: AtomicBool,
    /// Enabled state.
    enabled: AtomicBool,
}

impl SourceConnection {
    /// Creates a new source connection.
    #[must_use]
    pub fn new(id: ConnectionId, buffer_size: usize) -> Self {
        Self {
            id,
            buffer: RingBuffer::new(buffer_size),
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        }
    }

    /// Returns the connection ID.
    #[must_use]
    pub fn id(&self) -> &ConnectionId {
        &self.id
    }

    /// Writes samples to the connection buffer.
    ///
    /// Returns the number of samples written.
    pub fn write(&self, samples: &[Sample]) -> usize {
        self.buffer.write(samples)
    }

    /// Processes samples from the buffer into the output.
    ///
    /// Applies gain and mute. Returns the number of samples processed.
    pub fn process(&self, output: &mut [Sample]) -> usize {
        if !self.is_enabled() {
            output.fill(0.0);
            return output.len();
        }

        let read = self.buffer.read(output);

        if self.is_muted() {
            output[..read].fill(0.0);
        } else {
            let gain = self.gain();
            if (gain - 1.0).abs() > f32::EPSILON {
                for sample in &mut output[..read] {
                    *sample *= gain;
                }
            }
        }

        read
    }

    /// Gets the current gain.
    #[must_use]
    pub fn gain(&self) -> f32 {
        self.gain.get()
    }

    /// Sets the gain.
    pub fn set_gain(&self, gain: f32) {
        self.gain.set(gain);
    }

    /// Returns true if muted.
    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Sets the mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
    }

    /// Returns true if enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Sets the enabled state.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    /// Returns the number of available samples in the buffer.
    #[must_use]
    pub fn available(&self) -> usize {
        self.buffer.available()
    }

    /// Returns the buffer capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

#[cfg(test)]
mod connection_id_tests {
    use super::*;

    #[test]
    fn connection_id_new() {
        let id = ConnectionId::new("Node1", "Mic", 1, "Node2", "Speaker", 2);
        assert_eq!(id.source_node, "Node1");
        assert_eq!(id.source_device, "Mic");
        assert_eq!(id.source_channel, 1);
        assert_eq!(id.destination_node, "Node2");
        assert_eq!(id.destination_device, "Speaker");
        assert_eq!(id.destination_channel, 2);
    }

    #[test]
    fn connection_id_local() {
        let id = ConnectionId::local("Mic", 1, "Speaker", 2);
        assert_eq!(id.source_node, "LOCAL");
        assert_eq!(id.destination_node, "LOCAL");
        assert!(id.is_local());
    }

    #[test]
    fn connection_id_destination_id() {
        let id = ConnectionId::local("Mic", 1, "Speaker", 2);
        assert_eq!(id.destination_id(), "LOCAL:Speaker:2");
    }

    #[test]
    fn connection_id_source_id() {
        let id = ConnectionId::local("Mic", 1, "Speaker", 2);
        assert_eq!(id.source_id(), "LOCAL:Mic:1");
    }

    #[test]
    fn connection_id_display() {
        let id = ConnectionId::local("Mic", 1, "Speaker", 2);
        assert_eq!(id.to_string(), "LOCAL:Mic:1->LOCAL:Speaker:2");
    }

    #[test]
    fn connection_id_equality() {
        let id1 = ConnectionId::local("Mic", 1, "Speaker", 2);
        let id2 = ConnectionId::local("Mic", 1, "Speaker", 2);
        let id3 = ConnectionId::local("Mic", 2, "Speaker", 2);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
}

#[cfg(test)]
mod source_connection_tests {
    use super::*;

    fn test_id() -> ConnectionId {
        ConnectionId::local("Mic", 1, "Speaker", 1)
    }

    #[test]
    fn source_connection_new() {
        let conn = SourceConnection::new(test_id(), 1024);
        assert_eq!(conn.id(), &test_id());
        assert!((conn.gain() - 1.0).abs() < f32::EPSILON);
        assert!(!conn.is_muted());
        assert!(conn.is_enabled());
    }

    #[test]
    fn source_connection_write_read() {
        let conn = SourceConnection::new(test_id(), 1024);
        let input = [0.5; 64];
        let written = conn.write(&input);
        assert_eq!(written, 64);

        let mut output = [0.0; 64];
        let processed = conn.process(&mut output);
        assert_eq!(processed, 64);

        for sample in &output {
            assert!((*sample - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn source_connection_gain() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[1.0; 64]);

        conn.set_gain(0.5);
        assert!((conn.gain() - 0.5).abs() < f32::EPSILON);

        let mut output = [0.0; 64];
        conn.process(&mut output);

        for sample in &output {
            assert!((*sample - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn source_connection_mute() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[1.0; 64]);

        conn.set_muted(true);
        assert!(conn.is_muted());

        let mut output = [0.0; 64];
        conn.process(&mut output);

        for sample in &output {
            assert!(sample.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn source_connection_disabled() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[1.0; 64]);

        conn.set_enabled(false);
        assert!(!conn.is_enabled());

        let mut output = [1.0; 64]; // Fill with non-zero
        conn.process(&mut output);

        // Should be zeroed
        for sample in &output {
            assert!(sample.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn source_connection_buffer_stats() {
        let conn = SourceConnection::new(test_id(), 1024);
        assert_eq!(conn.capacity(), 1024);
        assert_eq!(conn.available(), 0);

        conn.write(&[0.5; 100]);
        assert_eq!(conn.available(), 100);
    }
}
