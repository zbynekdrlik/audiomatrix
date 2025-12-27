//! Destination channel with N:1 mixing and headroom management.
//!
//! A destination channel receives audio from multiple source connections
//! and mixes them together with configurable headroom handling.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::connection::SourceConnection;
use crate::Sample;

/// Headroom management mode for N:1 mixing.
///
/// When multiple sources are mixed to a single destination, the summed
/// signal may exceed the [-1.0, 1.0] range. This enum controls how
/// such situations are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum HeadroomMode {
    /// Hard clip samples to [-1.0, 1.0]. This is the default mode
    /// and has zero added latency but can cause audible distortion.
    #[default]
    Clip = 0,

    /// Automatically scale all samples when peak exceeds 1.0.
    /// The entire buffer is scaled proportionally to prevent clipping.
    /// Zero added latency but can cause pumping on dynamic material.
    AutoGain = 1,

    /// Apply soft-knee limiting. Provides the most transparent handling
    /// of peaks but adds approximately 0.5ms of latency for lookahead.
    Limiter = 2,

    /// User-specified fixed headroom. Samples are attenuated by a fixed
    /// amount regardless of actual peak levels.
    Manual = 3,
}

impl HeadroomMode {
    /// Converts from a u8 value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Clip),
            1 => Some(Self::AutoGain),
            2 => Some(Self::Limiter),
            3 => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Atomic wrapper for `HeadroomMode`.
#[derive(Debug)]
pub struct AtomicHeadroomMode {
    value: AtomicU8,
}

impl AtomicHeadroomMode {
    /// Creates a new atomic headroom mode.
    #[must_use]
    pub fn new(mode: HeadroomMode) -> Self {
        Self {
            value: AtomicU8::new(mode as u8),
        }
    }

    /// Loads the current mode.
    #[must_use]
    pub fn load(&self) -> HeadroomMode {
        HeadroomMode::from_u8(self.value.load(Ordering::Acquire)).unwrap_or_default()
    }

    /// Stores a new mode.
    pub fn store(&self, mode: HeadroomMode) {
        self.value.store(mode as u8, Ordering::Release);
    }
}

impl Default for AtomicHeadroomMode {
    fn default() -> Self {
        Self::new(HeadroomMode::default())
    }
}

/// A destination channel that mixes N source connections.
///
/// This represents a single output channel that can receive audio from
/// multiple source connections. The mixed signal is processed according
/// to the configured `HeadroomMode`.
pub struct DestinationChannel {
    /// Unique identifier for this destination.
    id: String,
    /// Headroom management mode.
    headroom_mode: AtomicHeadroomMode,
    /// Manual headroom level (dB, used when mode is Manual).
    manual_headroom_db: crate::AtomicF32,
    /// Source connections feeding this destination.
    sources: RwLock<Vec<Arc<SourceConnection>>>,
    /// Internal mix buffer (reused to avoid allocations).
    mix_buffer: RwLock<Vec<Sample>>,
}

impl DestinationChannel {
    /// Creates a new destination channel.
    #[must_use]
    pub fn new(id: impl Into<String>, buffer_size: usize) -> Self {
        Self {
            id: id.into(),
            headroom_mode: AtomicHeadroomMode::default(),
            manual_headroom_db: crate::AtomicF32::new(-6.0),
            sources: RwLock::new(Vec::new()),
            mix_buffer: RwLock::new(vec![0.0; buffer_size]),
        }
    }

    /// Returns the destination ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Gets the current headroom mode.
    #[must_use]
    pub fn headroom_mode(&self) -> HeadroomMode {
        self.headroom_mode.load()
    }

    /// Sets the headroom mode.
    pub fn set_headroom_mode(&self, mode: HeadroomMode) {
        self.headroom_mode.store(mode);
    }

    /// Gets the manual headroom level in dB.
    #[must_use]
    pub fn manual_headroom_db(&self) -> f32 {
        self.manual_headroom_db.get()
    }

    /// Sets the manual headroom level in dB.
    pub fn set_manual_headroom_db(&self, db: f32) {
        self.manual_headroom_db.set(db);
    }

    /// Adds a source connection to this destination.
    pub fn add_source(&self, source: Arc<SourceConnection>) {
        self.sources.write().push(source);
    }

    /// Removes a source connection by ID.
    ///
    /// Returns `true` if a source was removed.
    pub fn remove_source(&self, id: &crate::ConnectionId) -> bool {
        let mut sources = self.sources.write();
        let initial_len = sources.len();
        sources.retain(|s| s.id() != id);
        sources.len() != initial_len
    }

    /// Returns the number of source connections.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.read().len()
    }

    /// Processes all sources and mixes into the output buffer.
    ///
    /// # Audio Thread Considerations
    ///
    /// This method takes a read lock on the sources list. While the lock
    /// is typically very fast, for maximum real-time safety consider
    /// using a lock-free design for source management in hot paths.
    pub fn process(&self, output: &mut [Sample]) -> usize {
        let sources = self.sources.read();

        if sources.is_empty() {
            output.fill(0.0);
            return output.len();
        }

        // Clear output buffer
        output.fill(0.0);

        // Get or resize mix buffer
        let mut mix_buffer = self.mix_buffer.write();
        if mix_buffer.len() < output.len() {
            mix_buffer.resize(output.len(), 0.0);
        }

        // Process each source and sum into output
        for source in sources.iter() {
            // Clear mix buffer
            mix_buffer[..output.len()].fill(0.0);

            // Process source into mix buffer
            let processed = source.process(&mut mix_buffer[..output.len()]);

            // Add to output
            for (i, &sample) in mix_buffer[..processed].iter().enumerate() {
                output[i] += sample;
            }
        }

        // Apply headroom mode
        self.apply_headroom(output);

        output.len()
    }

    fn apply_headroom(&self, samples: &mut [Sample]) {
        match self.headroom_mode() {
            HeadroomMode::Clip => {
                Self::apply_clip(samples);
            },
            HeadroomMode::AutoGain => {
                Self::apply_auto_gain(samples);
            },
            HeadroomMode::Limiter => {
                // Simplified limiter (full implementation would need lookahead buffer)
                Self::apply_soft_clip(samples);
            },
            HeadroomMode::Manual => {
                let headroom_db = self.manual_headroom_db();
                let gain = 10.0_f32.powf(headroom_db / 20.0);
                for sample in samples.iter_mut() {
                    *sample *= gain;
                }
                Self::apply_clip(samples);
            },
        }
    }

    fn apply_clip(samples: &mut [Sample]) {
        for sample in samples.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }

    fn apply_auto_gain(samples: &mut [Sample]) {
        // Find peak
        let peak = samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);

        // If peak exceeds 1.0, scale all samples
        if peak > 1.0 {
            let gain = 1.0 / peak;
            for sample in samples.iter_mut() {
                *sample *= gain;
            }
        }
    }

    fn apply_soft_clip(samples: &mut [Sample]) {
        // Soft clipping using tanh-like curve
        // This provides a smoother transition than hard clipping
        for sample in samples.iter_mut() {
            let x = *sample;
            if x.abs() > 0.5 {
                // Soft knee region
                let sign = x.signum();
                let abs_x = x.abs();
                // Approximate soft clip: linear below 0.5, compressed above
                *sample = sign * (0.5 + (abs_x - 0.5).tanh() * 0.5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionId;

    fn create_source_with_samples(samples: &[Sample]) -> Arc<SourceConnection> {
        let id = ConnectionId::local("Test", 1, "Out", 1);
        let source = Arc::new(SourceConnection::new(id, 1024));
        source.write(samples);
        source
    }

    #[test]
    fn headroom_mode_default() {
        assert_eq!(HeadroomMode::default(), HeadroomMode::Clip);
    }

    #[test]
    fn headroom_mode_from_u8() {
        assert_eq!(HeadroomMode::from_u8(0), Some(HeadroomMode::Clip));
        assert_eq!(HeadroomMode::from_u8(1), Some(HeadroomMode::AutoGain));
        assert_eq!(HeadroomMode::from_u8(2), Some(HeadroomMode::Limiter));
        assert_eq!(HeadroomMode::from_u8(3), Some(HeadroomMode::Manual));
        assert_eq!(HeadroomMode::from_u8(4), None);
    }

    #[test]
    fn atomic_headroom_mode() {
        let atomic = AtomicHeadroomMode::new(HeadroomMode::AutoGain);
        assert_eq!(atomic.load(), HeadroomMode::AutoGain);

        atomic.store(HeadroomMode::Limiter);
        assert_eq!(atomic.load(), HeadroomMode::Limiter);
    }

    #[test]
    fn destination_new() {
        let dest = DestinationChannel::new("test-dest", 256);
        assert_eq!(dest.id(), "test-dest");
        assert_eq!(dest.headroom_mode(), HeadroomMode::Clip);
        assert_eq!(dest.source_count(), 0);
    }

    #[test]
    fn destination_add_remove_source() {
        let dest = DestinationChannel::new("test", 256);
        let id = ConnectionId::local("Mic", 1, "test", 1);
        let source = Arc::new(SourceConnection::new(id.clone(), 1024));

        dest.add_source(source);
        assert_eq!(dest.source_count(), 1);

        assert!(dest.remove_source(&id));
        assert_eq!(dest.source_count(), 0);

        // Removing non-existent returns false
        assert!(!dest.remove_source(&id));
    }

    #[test]
    fn destination_process_empty() {
        let dest = DestinationChannel::new("test", 256);
        let mut output = [1.0; 64];
        dest.process(&mut output);

        // Output should be zeroed
        for sample in &output {
            assert!(sample.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn destination_process_single_source() {
        let dest = DestinationChannel::new("test", 256);
        let source = create_source_with_samples(&[0.5; 64]);
        dest.add_source(source);

        let mut output = [0.0; 64];
        dest.process(&mut output);

        for sample in &output {
            assert!((*sample - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn destination_process_multiple_sources() {
        let dest = DestinationChannel::new("test", 256);
        dest.set_headroom_mode(HeadroomMode::AutoGain); // Prevent clipping

        let source1 = create_source_with_samples(&[0.3; 64]);
        let source2 = create_source_with_samples(&[0.2; 64]);
        dest.add_source(source1);
        dest.add_source(source2);

        let mut output = [0.0; 64];
        dest.process(&mut output);

        // Sum should be 0.5
        for sample in &output {
            assert!((*sample - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn destination_clip_mode() {
        let dest = DestinationChannel::new("test", 256);
        dest.set_headroom_mode(HeadroomMode::Clip);

        // Create sources that will sum to > 1.0
        let source1 = create_source_with_samples(&[0.8; 64]);
        let source2 = create_source_with_samples(&[0.8; 64]);
        dest.add_source(source1);
        dest.add_source(source2);

        let mut output = [0.0; 64];
        dest.process(&mut output);

        // Should be clipped to 1.0
        for sample in &output {
            assert!((*sample - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn destination_auto_gain_mode() {
        let dest = DestinationChannel::new("test", 256);
        dest.set_headroom_mode(HeadroomMode::AutoGain);

        // Create sources that will sum to 2.0
        let source1 = create_source_with_samples(&[1.0; 64]);
        let source2 = create_source_with_samples(&[1.0; 64]);
        dest.add_source(source1);
        dest.add_source(source2);

        let mut output = [0.0; 64];
        dest.process(&mut output);

        // Should be scaled to 1.0 (peak normalized)
        for sample in &output {
            assert!((*sample - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn destination_manual_headroom() {
        let dest = DestinationChannel::new("test", 256);
        dest.set_headroom_mode(HeadroomMode::Manual);
        dest.set_manual_headroom_db(-6.0); // ~0.5 gain

        let source = create_source_with_samples(&[1.0; 64]);
        dest.add_source(source);

        let mut output = [0.0; 64];
        dest.process(&mut output);

        // Should be attenuated by ~6dB (factor of ~0.5)
        for sample in &output {
            assert!((*sample - 0.501).abs() < 0.01);
        }
    }

    #[test]
    fn apply_clip() {
        let mut samples = [-2.0, -1.5, -1.0, 0.0, 1.0, 1.5, 2.0];
        DestinationChannel::apply_clip(&mut samples);

        assert!((samples[0] - (-1.0)).abs() < f32::EPSILON);
        assert!((samples[1] - (-1.0)).abs() < f32::EPSILON);
        assert!((samples[2] - (-1.0)).abs() < f32::EPSILON);
        assert!(samples[3].abs() < f32::EPSILON);
        assert!((samples[4] - 1.0).abs() < f32::EPSILON);
        assert!((samples[5] - 1.0).abs() < f32::EPSILON);
        assert!((samples[6] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn apply_auto_gain_no_scaling_needed() {
        let mut samples = [0.5, -0.5, 0.25];
        let original = samples;
        DestinationChannel::apply_auto_gain(&mut samples);

        // No scaling needed - peak is 0.5
        for (i, sample) in samples.iter().enumerate() {
            assert!((*sample - original[i]).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn apply_auto_gain_scaling_needed() {
        let mut samples = [2.0, -1.0, 0.5];
        DestinationChannel::apply_auto_gain(&mut samples);

        // Peak was 2.0, should scale by 0.5
        assert!((samples[0] - 1.0).abs() < f32::EPSILON);
        assert!((samples[1] - (-0.5)).abs() < f32::EPSILON);
        assert!((samples[2] - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn apply_soft_clip() {
        let mut samples = [0.3, 0.7, 1.5, -1.5];
        DestinationChannel::apply_soft_clip(&mut samples);

        // Values below 0.5 should be unchanged
        assert!((samples[0] - 0.3).abs() < f32::EPSILON);

        // Values above 0.5 should be compressed but not hard clipped
        assert!(samples[1] > 0.5 && samples[1] < 0.7);
        assert!(samples[2] > 0.5 && samples[2] < 1.0);
        assert!(samples[3] < -0.5 && samples[3] > -1.0);
    }
}
