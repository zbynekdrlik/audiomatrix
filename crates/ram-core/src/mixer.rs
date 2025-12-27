//! Audio mixing and volume control.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::Sample;

/// Converts a floating point volume (0.0 to 1.0+) to an atomic-friendly u32.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn volume_to_u32(volume: f32) -> u32 {
    (volume * 65536.0) as u32
}

/// Converts an atomic u32 back to a floating point volume.
#[allow(clippy::cast_precision_loss)]
fn u32_to_volume(value: u32) -> f32 {
    (value as f32) / 65536.0
}

/// Lock-free volume control for a single channel.
pub struct VolumeControl {
    volume: AtomicU32,
    muted: AtomicBool,
}

impl VolumeControl {
    /// Creates a new volume control with unity gain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            volume: AtomicU32::new(volume_to_u32(1.0)),
            muted: AtomicBool::new(false),
        }
    }

    /// Sets the volume level (0.0 = silence, 1.0 = unity, >1.0 = boost).
    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume_to_u32(volume.max(0.0)), Ordering::Release);
    }

    /// Gets the current volume level.
    #[must_use]
    pub fn volume(&self) -> f32 {
        u32_to_volume(self.volume.load(Ordering::Acquire))
    }

    /// Sets the mute state.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Release);
    }

    /// Returns whether the channel is muted.
    #[must_use]
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Acquire)
    }

    /// Returns the effective gain (0.0 if muted, otherwise volume).
    #[must_use]
    pub fn effective_gain(&self) -> f32 {
        if self.is_muted() {
            0.0
        } else {
            self.volume()
        }
    }

    /// Applies volume to a buffer of samples in-place.
    pub fn apply(&self, samples: &mut [Sample]) {
        let gain = self.effective_gain();
        if (gain - 1.0).abs() < f32::EPSILON {
            return; // Unity gain, no-op
        }
        for sample in samples {
            *sample *= gain;
        }
    }
}

impl Default for VolumeControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Mixes multiple input streams into a single output.
pub struct Mixer {
    headroom_db: f32,
}

impl Mixer {
    /// Creates a new mixer with the specified headroom.
    #[must_use]
    pub fn new(headroom_db: f32) -> Self {
        Self { headroom_db }
    }

    /// Returns the headroom as a linear gain multiplier.
    #[must_use]
    pub fn headroom_gain(&self) -> f32 {
        10.0_f32.powf(self.headroom_db / 20.0)
    }

    /// Mixes input buffers into the output buffer.
    ///
    /// The output buffer is first cleared, then all inputs are summed
    /// with headroom applied.
    pub fn mix(&self, inputs: &[&[Sample]], output: &mut [Sample]) {
        // Clear output
        output.fill(0.0);

        if inputs.is_empty() {
            return;
        }

        let headroom = self.headroom_gain();

        // Sum all inputs
        for input in inputs {
            let len = input.len().min(output.len());
            for (i, &sample) in input.iter().take(len).enumerate() {
                output[i] += sample * headroom;
            }
        }
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new(-6.0) // 6dB headroom by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_control_default_unity() {
        let vc = VolumeControl::new();
        assert!((vc.volume() - 1.0).abs() < 0.001);
        assert!(!vc.is_muted());
    }

    #[test]
    fn volume_control_mute() {
        let vc = VolumeControl::new();
        assert!((vc.effective_gain() - 1.0).abs() < 0.001);

        vc.set_muted(true);
        assert!(vc.effective_gain().abs() < 0.001);
    }

    #[test]
    fn mixer_sums_inputs() {
        let mixer = Mixer::new(0.0); // No headroom for test
        let input1 = [0.5_f32; 4];
        let input2 = [0.3_f32; 4];
        let mut output = [0.0_f32; 4];

        mixer.mix(&[&input1, &input2], &mut output);

        for sample in &output {
            assert!((*sample - 0.8).abs() < 0.001);
        }
    }
}
