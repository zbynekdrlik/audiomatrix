//! Audio connection with lock-free controls.
//!
//! A connection represents a single audio path from a source channel to a
//! destination channel. Each connection has independent gain, mute, and
//! enabled controls that can be modified from any thread without locking.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::atomic::AtomicF32;
use crate::buffer::RingBuffer;
use crate::Sample;

/// Unique identifier for a connection.
///
/// Format: `{src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionId {
    /// Source computer name (or "LOCAL" for local connections).
    pub src_computer: String,
    /// Source device name.
    pub src_device: String,
    /// Source channel (1-based).
    pub src_channel: u16,
    /// Destination computer name (or "LOCAL" for local connections).
    pub dst_computer: String,
    /// Destination device name.
    pub dst_device: String,
    /// Destination channel (1-based).
    pub dst_channel: u16,
}

impl ConnectionId {
    /// Creates a new connection ID.
    #[must_use]
    pub fn new(
        src_computer: impl Into<String>,
        src_device: impl Into<String>,
        src_channel: u16,
        dst_computer: impl Into<String>,
        dst_device: impl Into<String>,
        dst_channel: u16,
    ) -> Self {
        Self {
            src_computer: src_computer.into(),
            src_device: src_device.into(),
            src_channel,
            dst_computer: dst_computer.into(),
            dst_device: dst_device.into(),
            dst_channel,
        }
    }

    /// Creates a local connection ID (same computer).
    #[must_use]
    pub fn local(
        src_device: impl Into<String>,
        src_channel: u16,
        dst_device: impl Into<String>,
        dst_channel: u16,
    ) -> Self {
        Self::new(
            "LOCAL",
            src_device,
            src_channel,
            "LOCAL",
            dst_device,
            dst_channel,
        )
    }

    /// Returns the source part of the ID.
    #[must_use]
    pub fn source_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.src_computer, self.src_device, self.src_channel
        )
    }

    /// Returns the destination part of the ID.
    #[must_use]
    pub fn destination_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.dst_computer, self.dst_device, self.dst_channel
        )
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}>{}", self.source_id(), self.destination_id())
    }
}

impl std::str::FromStr for ConnectionId {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('>').collect();
        if parts.len() != 2 {
            return Err(crate::Error::InvalidConnectionId(s.to_string()));
        }

        let src_parts: Vec<&str> = parts[0].split(':').collect();
        let dst_parts: Vec<&str> = parts[1].split(':').collect();

        if src_parts.len() != 3 || dst_parts.len() != 3 {
            return Err(crate::Error::InvalidConnectionId(s.to_string()));
        }

        let src_channel = src_parts[2]
            .parse()
            .map_err(|_| crate::Error::InvalidConnectionId(s.to_string()))?;
        let dst_channel = dst_parts[2]
            .parse()
            .map_err(|_| crate::Error::InvalidConnectionId(s.to_string()))?;

        Ok(Self::new(
            src_parts[0],
            src_parts[1],
            src_channel,
            dst_parts[0],
            dst_parts[1],
            dst_channel,
        ))
    }
}

/// A source connection with lock-free audio controls.
///
/// This represents a single input to a destination channel. Multiple
/// `SourceConnection`s can feed into the same destination for N:1 mixing.
///
/// # Lock-Free Guarantees
///
/// All control operations (gain, mute, enabled) use atomic operations
/// and are safe to call from any thread, including the real-time audio
/// thread. The audio processing path never blocks.
///
/// # Enabled vs Muted
///
/// - `enabled=false`: Connection is completely skipped. No samples are
///   read from the source buffer, saving CPU.
/// - `muted=true`: Audio is processed (samples are read) but output is
///   silenced (zero samples). This maintains timing and allows for
///   instant unmute without audio discontinuities.
pub struct SourceConnection {
    /// Connection identifier.
    id: ConnectionId,
    /// Gain level (0.0 = silence, 1.0 = unity, >1.0 = boost).
    gain: AtomicF32,
    /// Mute state. When true, output is silenced but processing continues.
    muted: AtomicBool,
    /// Enable state. When false, connection is skipped entirely.
    enabled: AtomicBool,
    /// Version number for optimistic locking on concurrent updates.
    version: AtomicU64,
    /// Audio buffer for samples in transit.
    buffer: RingBuffer,
}

impl SourceConnection {
    /// Creates a new source connection with default settings.
    ///
    /// Default state:
    /// - Gain: 1.0 (unity)
    /// - Muted: false
    /// - Enabled: true
    #[must_use]
    pub fn new(id: ConnectionId, buffer_size: usize) -> Self {
        Self {
            id,
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
            version: AtomicU64::new(0),
            buffer: RingBuffer::new(buffer_size),
        }
    }

    /// Returns the connection ID.
    #[must_use]
    pub fn id(&self) -> &ConnectionId {
        &self.id
    }

    /// Gets the current gain level.
    #[must_use]
    pub fn gain(&self) -> f32 {
        self.gain.get()
    }

    /// Sets the gain level.
    ///
    /// Values are clamped to a minimum of 0.0.
    pub fn set_gain(&self, gain: f32) {
        self.gain.set(gain.max(0.0));
        self.increment_version();
    }

    /// Returns whether the connection is muted.
    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Sets the mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
        self.increment_version();
    }

    /// Returns whether the connection is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Sets the enabled state.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        self.increment_version();
    }

    /// Returns the current version number.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Returns the effective gain for audio processing.
    ///
    /// Returns 0.0 if muted, otherwise returns the gain value.
    /// Note: This does NOT check `enabled` - that should be checked
    /// separately to decide whether to process at all.
    #[must_use]
    pub fn effective_gain(&self) -> f32 {
        if self.is_muted() {
            0.0
        } else {
            self.gain()
        }
    }

    /// Returns a reference to the audio buffer.
    #[must_use]
    pub fn buffer(&self) -> &RingBuffer {
        &self.buffer
    }

    /// Processes audio from this connection into the output buffer.
    ///
    /// Returns the number of samples processed, or 0 if the connection
    /// is disabled.
    ///
    /// # Audio Thread Safety
    ///
    /// This method is lock-free and safe to call from the audio thread.
    pub fn process(&self, output: &mut [Sample]) -> usize {
        // Skip entirely if disabled
        if !self.is_enabled() {
            return 0;
        }

        let gain = self.effective_gain();

        // Read samples from buffer
        let read = self.buffer.read(output);

        // Apply gain
        if (gain - 1.0).abs() > f32::EPSILON {
            for sample in output.iter_mut().take(read) {
                *sample *= gain;
            }
        }

        read
    }

    /// Writes samples to this connection's buffer.
    ///
    /// Returns the number of samples written.
    pub fn write(&self, samples: &[Sample]) -> usize {
        self.buffer.write(samples)
    }

    fn increment_version(&self) {
        self.version.fetch_add(1, Ordering::Release);
    }
}

// SAFETY: All fields use atomic types or are immutable
unsafe impl Send for SourceConnection {}
unsafe impl Sync for SourceConnection {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id() -> ConnectionId {
        ConnectionId::local("Mic", 1, "Speaker", 1)
    }

    #[test]
    fn connection_id_display() {
        let id = ConnectionId::new("STUDIO", "Focusrite", 1, "LIVE", "VASIO", 2);
        assert_eq!(id.to_string(), "STUDIO:Focusrite:1>LIVE:VASIO:2");
    }

    #[test]
    fn connection_id_local() {
        let id = ConnectionId::local("Mic", 1, "Speaker", 2);
        assert_eq!(id.src_computer, "LOCAL");
        assert_eq!(id.dst_computer, "LOCAL");
        assert_eq!(id.to_string(), "LOCAL:Mic:1>LOCAL:Speaker:2");
    }

    #[test]
    fn connection_id_parse() {
        let id: ConnectionId = "STUDIO:Focusrite:1>LIVE:VASIO:2".parse().unwrap();
        assert_eq!(id.src_computer, "STUDIO");
        assert_eq!(id.src_device, "Focusrite");
        assert_eq!(id.src_channel, 1);
        assert_eq!(id.dst_computer, "LIVE");
        assert_eq!(id.dst_device, "VASIO");
        assert_eq!(id.dst_channel, 2);
    }

    #[test]
    fn connection_id_parse_invalid() {
        assert!("invalid".parse::<ConnectionId>().is_err());
        assert!("a>b".parse::<ConnectionId>().is_err());
        assert!("a:b:c".parse::<ConnectionId>().is_err());
        assert!("a:b:x>c:d:y".parse::<ConnectionId>().is_err());
    }

    #[test]
    fn connection_id_parts() {
        let id = ConnectionId::new("A", "B", 1, "C", "D", 2);
        assert_eq!(id.source_id(), "A:B:1");
        assert_eq!(id.destination_id(), "C:D:2");
    }

    #[test]
    fn new_connection_defaults() {
        let conn = SourceConnection::new(test_id(), 1024);
        assert!((conn.gain() - 1.0).abs() < f32::EPSILON);
        assert!(!conn.is_muted());
        assert!(conn.is_enabled());
        assert_eq!(conn.version(), 0);
    }

    #[test]
    fn set_gain() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.set_gain(0.5);
        assert!((conn.gain() - 0.5).abs() < f32::EPSILON);
        assert_eq!(conn.version(), 1);

        // Negative values clamp to 0
        conn.set_gain(-1.0);
        assert!(conn.gain().abs() < f32::EPSILON);
    }

    #[test]
    fn set_muted() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.set_muted(true);
        assert!(conn.is_muted());
        assert_eq!(conn.version(), 1);

        conn.set_muted(false);
        assert!(!conn.is_muted());
        assert_eq!(conn.version(), 2);
    }

    #[test]
    fn set_enabled() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.set_enabled(false);
        assert!(!conn.is_enabled());
        assert_eq!(conn.version(), 1);
    }

    #[test]
    fn effective_gain_with_mute() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.set_gain(0.5);
        assert!((conn.effective_gain() - 0.5).abs() < f32::EPSILON);

        conn.set_muted(true);
        assert!(conn.effective_gain().abs() < f32::EPSILON);
    }

    #[test]
    fn process_disabled_returns_zero() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[0.5; 64]);
        conn.set_enabled(false);

        let mut output = [0.0; 64];
        let processed = conn.process(&mut output);
        assert_eq!(processed, 0);
    }

    #[test]
    fn process_applies_gain() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[1.0; 64]);
        conn.set_gain(0.5);

        let mut output = [0.0; 64];
        let processed = conn.process(&mut output);
        assert_eq!(processed, 64);
        for sample in &output {
            assert!((*sample - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn process_muted_outputs_silence() {
        let conn = SourceConnection::new(test_id(), 1024);
        conn.write(&[1.0; 64]);
        conn.set_muted(true);

        let mut output = [0.0; 64];
        let processed = conn.process(&mut output);
        assert_eq!(processed, 64);
        for sample in &output {
            assert!(sample.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn process_unity_gain_passthrough() {
        let conn = SourceConnection::new(test_id(), 1024);
        let input = [0.75; 64];
        conn.write(&input);

        let mut output = [0.0; 64];
        let processed = conn.process(&mut output);
        assert_eq!(processed, 64);
        for sample in &output {
            assert!((*sample - 0.75).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn version_increments_on_changes() {
        let conn = SourceConnection::new(test_id(), 1024);
        assert_eq!(conn.version(), 0);

        conn.set_gain(0.5);
        assert_eq!(conn.version(), 1);

        conn.set_muted(true);
        assert_eq!(conn.version(), 2);

        conn.set_enabled(false);
        assert_eq!(conn.version(), 3);
    }

    #[test]
    fn buffer_access() {
        let conn = SourceConnection::new(test_id(), 1024);
        assert_eq!(conn.buffer().available(), 0);
        conn.write(&[0.5; 32]);
        assert_eq!(conn.buffer().available(), 32);
    }
}
