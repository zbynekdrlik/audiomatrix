//! Adaptive jitter buffer for VBAN audio reception.
//!
//! This module provides a jitter buffer that dynamically adjusts its size
//! based on network conditions to minimize latency while preventing underruns.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use ram_core::Sample;

/// Configuration for the jitter buffer.
#[derive(Debug, Clone)]
pub struct JitterBufferConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of audio channels.
    pub channels: usize,
    /// Minimum buffer size in milliseconds.
    pub min_buffer_ms: f32,
    /// Maximum buffer size in milliseconds.
    pub max_buffer_ms: f32,
    /// Initial buffer size in milliseconds.
    pub initial_buffer_ms: f32,
    /// Timeout in milliseconds before marking stream offline.
    pub timeout_ms: u32,
}

impl Default for JitterBufferConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            min_buffer_ms: 1.0,
            max_buffer_ms: 5.0,
            initial_buffer_ms: 2.0,
            timeout_ms: 100,
        }
    }
}

impl JitterBufferConfig {
    /// Converts milliseconds to samples.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn ms_to_samples(&self, ms: f32) -> usize {
        ((ms / 1000.0) * self.sample_rate as f32) as usize * self.channels
    }

    /// Returns minimum buffer size in samples.
    #[must_use]
    pub fn min_samples(&self) -> usize {
        self.ms_to_samples(self.min_buffer_ms)
    }

    /// Returns maximum buffer size in samples.
    #[must_use]
    pub fn max_samples(&self) -> usize {
        self.ms_to_samples(self.max_buffer_ms)
    }

    /// Returns initial buffer size in samples.
    #[must_use]
    pub fn initial_samples(&self) -> usize {
        self.ms_to_samples(self.initial_buffer_ms)
    }
}

/// A packet in the jitter buffer.
#[derive(Debug, Clone)]
pub struct BufferedPacket {
    /// Sequence number (frame counter).
    pub sequence: u32,
    /// Audio samples.
    pub samples: Vec<Sample>,
}

/// Statistics for the jitter buffer.
#[derive(Debug, Default)]
pub struct JitterStats {
    /// Total packets received.
    packets_received: AtomicU64,
    /// Packets that arrived out of order.
    packets_reordered: AtomicU64,
    /// Packets that were dropped (too late).
    packets_dropped: AtomicU64,
    /// Buffer underrun count.
    underruns: AtomicU64,
    /// Last received sequence number.
    last_sequence: AtomicU32,
    /// Sequence gaps filled with silence.
    gaps_filled: AtomicU64,
}

impl JitterStats {
    /// Returns total packets received.
    #[must_use]
    pub fn packets_received(&self) -> u64 {
        self.packets_received.load(Ordering::Relaxed)
    }

    /// Returns packets that arrived out of order.
    #[must_use]
    pub fn packets_reordered(&self) -> u64 {
        self.packets_reordered.load(Ordering::Relaxed)
    }

    /// Returns packets that were dropped.
    #[must_use]
    pub fn packets_dropped(&self) -> u64 {
        self.packets_dropped.load(Ordering::Relaxed)
    }

    /// Returns buffer underrun count.
    #[must_use]
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Returns gaps filled with silence.
    #[must_use]
    pub fn gaps_filled(&self) -> u64 {
        self.gaps_filled.load(Ordering::Relaxed)
    }

    /// Returns the packet loss rate (0.0 - 1.0).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn loss_rate(&self) -> f64 {
        let received = self.packets_received();
        let dropped = self.packets_dropped();
        if received + dropped == 0 {
            0.0
        } else {
            dropped as f64 / (received + dropped) as f64
        }
    }

    fn increment_received(&self) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_reordered(&self) {
        self.packets_reordered.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_dropped(&self) {
        self.packets_dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_underruns(&self) {
        self.underruns.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_gaps_filled(&self) {
        self.gaps_filled.fetch_add(1, Ordering::Relaxed);
    }

    fn update_last_sequence(&self, seq: u32) {
        self.last_sequence.store(seq, Ordering::Relaxed);
    }
}

/// Adaptive jitter buffer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JitterBufferState {
    /// Buffering initial data.
    Buffering,
    /// Normal playback.
    Playing,
    /// Stream is offline (no packets received within timeout).
    Offline,
}

/// An adaptive jitter buffer for receiving VBAN audio.
///
/// The buffer dynamically adjusts its size based on observed jitter
/// to minimize latency while preventing underruns.
pub struct JitterBuffer {
    /// Configuration.
    config: JitterBufferConfig,
    /// Buffered packets waiting to be played.
    packets: VecDeque<BufferedPacket>,
    /// Current target buffer size in samples.
    target_size: usize,
    /// Next expected sequence number.
    next_sequence: Option<u32>,
    /// Current state.
    state: JitterBufferState,
    /// Statistics.
    stats: JitterStats,
    /// Samples available for reading.
    sample_buffer: VecDeque<Sample>,
    /// Jitter measurements for adaptation (in samples).
    jitter_samples: VecDeque<i32>,
    /// Maximum jitter window for adaptation.
    max_jitter_window: usize,
}

impl JitterBuffer {
    /// Creates a new jitter buffer with the given configuration.
    #[must_use]
    pub fn new(config: JitterBufferConfig) -> Self {
        let target_size = config.initial_samples();
        let max_samples = config.max_samples();

        Self {
            config,
            packets: VecDeque::with_capacity(32),
            target_size,
            next_sequence: None,
            state: JitterBufferState::Buffering,
            stats: JitterStats::default(),
            sample_buffer: VecDeque::with_capacity(max_samples * 2),
            jitter_samples: VecDeque::with_capacity(64),
            max_jitter_window: 64,
        }
    }

    /// Creates a jitter buffer with default configuration.
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(JitterBufferConfig::default())
    }

    /// Returns the current state.
    #[must_use]
    pub fn state(&self) -> JitterBufferState {
        self.state
    }

    /// Returns the current target buffer size in samples.
    #[must_use]
    pub fn target_size(&self) -> usize {
        self.target_size
    }

    /// Returns the current buffer level in samples.
    #[must_use]
    pub fn level(&self) -> usize {
        self.sample_buffer.len()
    }

    /// Returns the buffer statistics.
    #[must_use]
    pub fn stats(&self) -> &JitterStats {
        &self.stats
    }

    /// Adds a packet to the jitter buffer.
    ///
    /// Packets may arrive out of order; the buffer will reorder them
    /// based on sequence numbers.
    ///
    /// # Panics
    ///
    /// This function will not panic under normal use. The internal `expect`
    /// is guaranteed to succeed because `next_sequence` is set on the first
    /// packet if it was `None`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn push(&mut self, sequence: u32, samples: Vec<Sample>) {
        self.stats.increment_received();
        self.stats.update_last_sequence(sequence);

        // Handle first packet
        if self.next_sequence.is_none() {
            self.next_sequence = Some(sequence);
            self.state = JitterBufferState::Buffering;
        }

        // SAFETY: We just set next_sequence if it was None
        let next_seq = self.next_sequence.expect("next_sequence was just set");

        // Check if packet is too old (already played)
        if Self::sequence_diff(sequence, next_seq) < 0 {
            self.stats.increment_dropped();
            return;
        }

        // Calculate jitter for adaptation
        if let Some(&last_seq) = self.packets.back().map(|p| &p.sequence) {
            let expected_diff = 1i32;
            let actual_diff = Self::sequence_diff(sequence, last_seq);
            let jitter = (actual_diff - expected_diff).abs();
            self.record_jitter(jitter.saturating_mul(samples.len() as i32));
        }

        // Insert in sequence order
        let packet = BufferedPacket { sequence, samples };
        let insert_pos = self
            .packets
            .iter()
            .position(|p| Self::sequence_diff(sequence, p.sequence) < 0)
            .unwrap_or(self.packets.len());

        if insert_pos < self.packets.len() {
            self.stats.increment_reordered();
        }

        self.packets.insert(insert_pos, packet);

        // Process packets in order
        self.process_packets();

        // Check if we have enough data to start playing
        if self.state == JitterBufferState::Buffering
            && self.sample_buffer.len() >= self.target_size
        {
            self.state = JitterBufferState::Playing;
        }
    }

    /// Reads samples from the buffer.
    ///
    /// Returns the number of samples read. If insufficient samples are
    /// available, the output is filled with silence.
    pub fn read(&mut self, output: &mut [Sample]) -> usize {
        if self.state == JitterBufferState::Buffering {
            // Still buffering, output silence
            output.fill(0.0);
            return 0;
        }

        let available = self.sample_buffer.len();
        let to_read = output.len();

        if available >= to_read {
            // Normal case: enough samples available
            for sample in output.iter_mut() {
                *sample = self.sample_buffer.pop_front().unwrap_or(0.0);
            }
            to_read
        } else {
            // Underrun: output available samples then silence
            self.stats.increment_underruns();
            for (i, sample) in output.iter_mut().enumerate() {
                *sample = if i < available {
                    self.sample_buffer.pop_front().unwrap_or(0.0)
                } else {
                    0.0
                };
            }

            // Increase buffer size due to underrun
            self.increase_buffer_size();

            available
        }
    }

    /// Marks the stream as offline.
    pub fn set_offline(&mut self) {
        self.state = JitterBufferState::Offline;
    }

    /// Resets the buffer state.
    pub fn reset(&mut self) {
        self.packets.clear();
        self.sample_buffer.clear();
        self.next_sequence = None;
        self.state = JitterBufferState::Buffering;
        self.target_size = self.config.initial_samples();
        self.jitter_samples.clear();
    }

    /// Returns the current buffer size in milliseconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn buffer_ms(&self) -> f32 {
        let samples = self.sample_buffer.len();
        let samples_per_ms =
            (self.config.sample_rate as f32 / 1000.0) * self.config.channels as f32;
        samples as f32 / samples_per_ms
    }

    #[allow(clippy::cast_possible_wrap)]
    fn sequence_diff(a: u32, b: u32) -> i32 {
        // Handle wraparound for 32-bit sequence numbers
        let diff = a.wrapping_sub(b);
        if diff > 0x8000_0000 {
            -((!diff).wrapping_add(1) as i32)
        } else {
            diff as i32
        }
    }

    #[allow(clippy::cast_sign_loss)]
    fn process_packets(&mut self) {
        while let Some(packet) = self.packets.front() {
            let next_seq = self.next_sequence.unwrap_or(0);
            let diff = Self::sequence_diff(packet.sequence, next_seq);

            if diff < 0 {
                // Packet is old, skip it
                self.packets.pop_front();
                continue;
            }

            if diff == 0 {
                // Next expected packet (pop is guaranteed to succeed since we just checked front)
                if let Some(packet) = self.packets.pop_front() {
                    self.sample_buffer.extend(packet.samples);
                    self.next_sequence = Some(next_seq.wrapping_add(1));
                }
            } else if diff > 0 && diff <= 3 && self.packets.len() > 2 {
                // Small gap with enough buffered packets - fill with silence
                // Only fill gaps when we have some buffer to work with
                let gap_samples =
                    diff as usize * self.packets.front().map_or(256, |p| p.samples.len());
                for _ in 0..gap_samples {
                    self.sample_buffer.push_back(0.0);
                }
                self.stats.increment_gaps_filled();
                self.next_sequence = Some(next_seq.wrapping_add(diff as u32));
            } else {
                // Wait for more packets (either large gap or not enough buffered)
                break;
            }
        }
    }

    fn record_jitter(&mut self, jitter: i32) {
        if self.jitter_samples.len() >= self.max_jitter_window {
            self.jitter_samples.pop_front();
        }
        self.jitter_samples.push_back(jitter);
        self.adapt_buffer_size();
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn adapt_buffer_size(&mut self) {
        if self.jitter_samples.len() < 16 {
            return; // Not enough data
        }

        // Calculate 95th percentile jitter
        let mut sorted: Vec<i32> = self.jitter_samples.iter().copied().collect();
        sorted.sort_unstable();
        let p95_index = (sorted.len() as f32 * 0.95) as usize;
        let p95_jitter = sorted.get(p95_index).copied().unwrap_or(0);

        // Target buffer should cover the 95th percentile jitter
        let new_target = (p95_jitter.max(0) as usize)
            .clamp(self.config.min_samples(), self.config.max_samples());

        // Gradual adaptation
        if new_target > self.target_size {
            self.target_size = self.target_size + (new_target - self.target_size) / 4;
        } else if new_target < self.target_size {
            self.target_size = self.target_size - (self.target_size - new_target) / 8;
        }
    }

    fn increase_buffer_size(&mut self) {
        let increase = self.config.min_samples();
        self.target_size = (self.target_size + increase).min(self.config.max_samples());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JitterBufferConfig {
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
    fn config_ms_to_samples() {
        let config = test_config();
        // 1ms at 48000Hz stereo = 96 samples
        assert_eq!(config.ms_to_samples(1.0), 96);
        // 2ms = 192 samples
        assert_eq!(config.ms_to_samples(2.0), 192);
    }

    #[test]
    fn jitter_buffer_new() {
        let buffer = JitterBuffer::new(test_config());
        assert_eq!(buffer.state(), JitterBufferState::Buffering);
        assert_eq!(buffer.level(), 0);
    }

    #[test]
    fn jitter_buffer_push_single() {
        let mut buffer = JitterBuffer::new(test_config());
        let samples = vec![0.5; 256];
        buffer.push(0, samples.clone());

        assert_eq!(buffer.stats().packets_received(), 1);
        assert_eq!(buffer.level(), 256);
    }

    #[test]
    fn jitter_buffer_push_sequence() {
        let mut buffer = JitterBuffer::new(test_config());

        for i in 0..4 {
            buffer.push(i, vec![0.5; 64]);
        }

        assert_eq!(buffer.stats().packets_received(), 4);
        assert_eq!(buffer.level(), 256);
    }

    #[test]
    fn jitter_buffer_reorder() {
        let mut buffer = JitterBuffer::new(test_config());

        // Push out of order: 0, then 2, then 1
        buffer.push(0, vec![0.1; 64]);
        buffer.push(2, vec![0.3; 64]);
        buffer.push(1, vec![0.2; 64]); // This one is reordered

        assert_eq!(buffer.stats().packets_received(), 3);
        assert_eq!(buffer.stats().packets_reordered(), 1);

        // Should be reordered correctly - first 64 samples should be 0.1
        let mut output = [0.0; 64];
        buffer.state = JitterBufferState::Playing;
        buffer.read(&mut output);
        assert!((output[0] - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn jitter_buffer_drop_old() {
        let mut buffer = JitterBuffer::new(test_config());

        buffer.push(5, vec![0.5; 64]);
        buffer.push(3, vec![0.3; 64]); // Old packet

        assert_eq!(buffer.stats().packets_dropped(), 1);
    }

    #[test]
    fn jitter_buffer_read_underrun() {
        let mut buffer = JitterBuffer::new(test_config());
        buffer.push(0, vec![0.5; 32]);
        buffer.state = JitterBufferState::Playing;

        let mut output = [0.0; 64];
        let read = buffer.read(&mut output);

        assert_eq!(read, 32); // Only 32 samples available
        assert_eq!(buffer.stats().underruns(), 1);

        // First 32 should be 0.5, rest should be silence
        for (i, &sample) in output.iter().enumerate() {
            if i < 32 {
                assert!((sample - 0.5).abs() < f32::EPSILON);
            } else {
                assert!(sample.abs() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn jitter_buffer_buffering_state() {
        let config = JitterBufferConfig {
            initial_buffer_ms: 2.0,
            sample_rate: 48000,
            channels: 2,
            ..test_config()
        };
        let mut buffer = JitterBuffer::new(config);

        // Push less than target
        buffer.push(0, vec![0.5; 64]);
        assert_eq!(buffer.state(), JitterBufferState::Buffering);

        // Push enough to meet target
        buffer.push(1, vec![0.5; 192]);
        assert_eq!(buffer.state(), JitterBufferState::Playing);
    }

    #[test]
    fn jitter_buffer_reset() {
        let mut buffer = JitterBuffer::new(test_config());
        buffer.push(0, vec![0.5; 64]);
        buffer.state = JitterBufferState::Playing;

        buffer.reset();

        assert_eq!(buffer.state(), JitterBufferState::Buffering);
        assert_eq!(buffer.level(), 0);
    }

    #[test]
    fn jitter_buffer_sequence_wraparound() {
        let mut buffer = JitterBuffer::new(test_config());

        // Test near wraparound
        buffer.push(u32::MAX - 1, vec![0.1; 64]);
        buffer.push(u32::MAX, vec![0.2; 64]);
        buffer.push(0, vec![0.3; 64]);
        buffer.push(1, vec![0.4; 64]);

        assert_eq!(buffer.stats().packets_received(), 4);
        assert_eq!(buffer.level(), 256);
    }

    #[test]
    fn stats_loss_rate() {
        let stats = JitterStats::default();
        assert!((stats.loss_rate() - 0.0).abs() < f64::EPSILON);

        for _ in 0..80 {
            stats.increment_received();
        }
        for _ in 0..20 {
            stats.increment_dropped();
        }

        let rate = stats.loss_rate();
        assert!((rate - 0.2).abs() < 0.01); // ~20% loss
    }

    #[test]
    fn jitter_buffer_gap_filling() {
        let mut buffer = JitterBuffer::new(test_config());

        // Push enough packets so gap filling is triggered
        // (requires packets.len() > 2 before filling gaps)
        buffer.push(0, vec![0.1; 64]);
        buffer.push(2, vec![0.3; 64]); // Skip sequence 1
        buffer.push(3, vec![0.4; 64]);
        buffer.push(4, vec![0.5; 64]); // Now we have enough packets

        // Gap should be detected and filled
        assert!(buffer.stats().gaps_filled() >= 1);
    }
}
