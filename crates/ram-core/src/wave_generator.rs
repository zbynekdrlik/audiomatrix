//! Sync wave generator for test signal injection.
//!
//! Provides various waveform types for testing and channel identification.
//! All generators are lock-free and suitable for real-time audio callbacks.

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Waveform types for test signal generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WaveformType {
    /// Sine wave at specified frequency.
    #[default]
    Sine = 0,
    /// Pink noise (1/f spectrum).
    PinkNoise = 1,
    /// Channel ID tone - unique frequency per channel.
    ChannelId = 2,
    /// Frequency sweep (20Hz - 20kHz).
    Sweep = 3,
    /// Click/impulse (single sample pulse).
    Click = 4,
}

impl WaveformType {
    /// Convert from u8 value.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Sine),
            1 => Some(Self::PinkNoise),
            2 => Some(Self::ChannelId),
            3 => Some(Self::Sweep),
            4 => Some(Self::Click),
            _ => None,
        }
    }
}

/// Maximum output level in dBFS (safety limit).
pub const MAX_LEVEL_DBFS: f32 = -6.0;

/// Default output level in dBFS.
pub const DEFAULT_LEVEL_DBFS: f32 = -18.0;

/// Configuration for a wave generator (lock-free, atomic).
///
/// This struct is designed to be modified from any thread while
/// being read from the audio callback without locks.
#[derive(Debug)]
pub struct WaveGeneratorConfig {
    /// Whether the generator is enabled.
    enabled: AtomicBool,
    /// Waveform type (as u8).
    waveform: AtomicU32,
    /// Frequency in Hz (for applicable waveforms).
    frequency: AtomicU32,
    /// Output level in dBFS * 100 (to store as integer).
    /// Stored as i32 to handle negative values.
    level_db_x100: AtomicU32,
}

impl Default for WaveGeneratorConfig {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            waveform: AtomicU32::new(WaveformType::Sine as u32),
            frequency: AtomicU32::new(1000), // 1kHz default
            level_db_x100: AtomicU32::new((-DEFAULT_LEVEL_DBFS * 100.0) as u32), // Store as positive
        }
    }
}

impl WaveGeneratorConfig {
    /// Create a new generator config with specified settings.
    pub fn new(waveform: WaveformType, frequency: u32, level_db: f32) -> Self {
        // Clamp to at most MAX_LEVEL_DBFS (-6.0 dB) - can't be louder than this
        let clamped_level = level_db.min(MAX_LEVEL_DBFS);
        Self {
            enabled: AtomicBool::new(false),
            waveform: AtomicU32::new(waveform as u32),
            frequency: AtomicU32::new(frequency),
            level_db_x100: AtomicU32::new((-clamped_level * 100.0) as u32),
        }
    }

    /// Check if generator is enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Enable or disable the generator.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Get the current waveform type.
    #[inline]
    pub fn waveform(&self) -> WaveformType {
        WaveformType::from_u8(self.waveform.load(Ordering::Relaxed) as u8)
            .unwrap_or(WaveformType::Sine)
    }

    /// Set the waveform type.
    pub fn set_waveform(&self, waveform: WaveformType) {
        self.waveform.store(waveform as u32, Ordering::Relaxed);
    }

    /// Get the frequency in Hz.
    #[inline]
    pub fn frequency(&self) -> u32 {
        self.frequency.load(Ordering::Relaxed)
    }

    /// Set the frequency in Hz.
    pub fn set_frequency(&self, freq: u32) {
        self.frequency.store(freq, Ordering::Relaxed);
    }

    /// Get the output level in dBFS.
    #[inline]
    pub fn level_db(&self) -> f32 {
        -(self.level_db_x100.load(Ordering::Relaxed) as f32 / 100.0)
    }

    /// Set the output level in dBFS (clamped to at most MAX_LEVEL_DBFS).
    /// Values louder than -6.0 dBFS are clamped for safety.
    pub fn set_level_db(&self, level_db: f32) {
        // Clamp to at most MAX_LEVEL_DBFS (-6.0 dB) - can't be louder than this
        let clamped = level_db.min(MAX_LEVEL_DBFS);
        self.level_db_x100
            .store((-clamped * 100.0) as u32, Ordering::Relaxed);
    }

    /// Get the linear gain (0.0 - 1.0).
    #[inline]
    pub fn linear_gain(&self) -> f32 {
        let db = self.level_db();
        10.0f32.powf(db / 20.0)
    }
}

/// Wave generator state for a single channel.
///
/// Maintains phase accumulator and other state needed for
/// continuous waveform generation.
#[derive(Debug, Default)]
pub struct WaveGeneratorState {
    /// Phase accumulator (0.0 - 1.0).
    phase: f32,
    /// Pink noise state (Voss-McCartney algorithm).
    pink_state: PinkNoiseState,
    /// Sweep phase (0.0 - 1.0 over sweep duration).
    sweep_phase: f32,
    /// Click counter (counts down samples until next click).
    click_counter: u32,
    /// Random state for noise generation.
    random_state: u32,
}

/// Pink noise generator state using Voss-McCartney algorithm.
#[derive(Debug, Default)]
struct PinkNoiseState {
    /// Octave band values.
    octaves: [f32; 8],
    /// Counter for octave update scheduling.
    counter: u32,
}

impl WaveGeneratorState {
    /// Create a new generator state.
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            pink_state: PinkNoiseState::default(),
            sweep_phase: 0.0,
            click_counter: 0,
            random_state: 0x1234_5678,
        }
    }

    /// Create a new generator state with channel-specific initialization.
    pub fn for_channel(channel: u16) -> Self {
        Self {
            phase: 0.0,
            pink_state: PinkNoiseState::default(),
            sweep_phase: 0.0,
            click_counter: 0,
            // Seed random state based on channel for varied noise
            // Use wrapping_mul to intentionally allow overflow for hash mixing
            random_state: 0x1234_5678 ^ (channel as u32).wrapping_mul(0x9E37_79B9),
        }
    }

    /// Generate a single sample.
    ///
    /// # Arguments
    /// * `config` - Generator configuration
    /// * `sample_rate` - Current sample rate
    /// * `channel` - Channel number (1-based, for ChannelId waveform)
    #[inline]
    pub fn generate_sample(
        &mut self,
        config: &WaveGeneratorConfig,
        sample_rate: u32,
        channel: u16,
    ) -> f32 {
        if !config.is_enabled() {
            return 0.0;
        }

        let raw_sample = match config.waveform() {
            WaveformType::Sine => self.generate_sine(config.frequency(), sample_rate),
            WaveformType::PinkNoise => self.generate_pink_noise(),
            WaveformType::ChannelId => self.generate_channel_id(channel, sample_rate),
            WaveformType::Sweep => self.generate_sweep(sample_rate),
            WaveformType::Click => self.generate_click(sample_rate),
        };

        raw_sample * config.linear_gain()
    }

    /// Generate a block of samples (more efficient than per-sample).
    pub fn generate_block(
        &mut self,
        config: &WaveGeneratorConfig,
        sample_rate: u32,
        channel: u16,
        output: &mut [f32],
    ) {
        if !config.is_enabled() {
            // Zero the output buffer
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
            return;
        }

        let gain = config.linear_gain();
        let waveform = config.waveform();

        for sample in output.iter_mut() {
            let raw = match waveform {
                WaveformType::Sine => self.generate_sine(config.frequency(), sample_rate),
                WaveformType::PinkNoise => self.generate_pink_noise(),
                WaveformType::ChannelId => self.generate_channel_id(channel, sample_rate),
                WaveformType::Sweep => self.generate_sweep(sample_rate),
                WaveformType::Click => self.generate_click(sample_rate),
            };
            *sample = raw * gain;
        }
    }

    /// Generate a block and add to existing samples (mixing).
    pub fn mix_into_block(
        &mut self,
        config: &WaveGeneratorConfig,
        sample_rate: u32,
        channel: u16,
        output: &mut [f32],
    ) {
        if !config.is_enabled() {
            return;
        }

        let gain = config.linear_gain();
        let waveform = config.waveform();

        for sample in output.iter_mut() {
            let raw = match waveform {
                WaveformType::Sine => self.generate_sine(config.frequency(), sample_rate),
                WaveformType::PinkNoise => self.generate_pink_noise(),
                WaveformType::ChannelId => self.generate_channel_id(channel, sample_rate),
                WaveformType::Sweep => self.generate_sweep(sample_rate),
                WaveformType::Click => self.generate_click(sample_rate),
            };
            *sample += raw * gain;
        }
    }

    // Internal generation methods

    #[inline]
    fn generate_sine(&mut self, frequency: u32, sample_rate: u32) -> f32 {
        let phase_inc = frequency as f32 / sample_rate as f32;
        let sample = (self.phase * 2.0 * PI).sin();
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        sample
    }

    #[inline]
    fn generate_pink_noise(&mut self) -> f32 {
        // Voss-McCartney algorithm for pink noise
        self.pink_state.counter = self.pink_state.counter.wrapping_add(1);

        // Determine which octaves to update based on trailing zeros
        let zeros = self.pink_state.counter.trailing_zeros() as usize;
        if zeros < 8 {
            // Generate white noise inline to avoid borrow issues
            self.random_state ^= self.random_state << 13;
            self.random_state ^= self.random_state >> 17;
            self.random_state ^= self.random_state << 5;
            let noise = (self.random_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            self.pink_state.octaves[zeros] = noise;
        }

        // Sum all octaves
        let sum: f32 = self.pink_state.octaves.iter().sum();
        // Normalize (8 octaves, each in -1..1, so max sum is 8)
        sum / 8.0
    }

    #[inline]
    fn generate_channel_id(&mut self, channel: u16, sample_rate: u32) -> f32 {
        // Generate unique frequency per channel
        // Base: 200Hz, increment: 100Hz per channel
        // Channel 1 = 200Hz, Channel 2 = 300Hz, etc.
        let frequency = 200 + (channel.saturating_sub(1) as u32 * 100);
        self.generate_sine(frequency, sample_rate)
    }

    #[inline]
    fn generate_sweep(&mut self, sample_rate: u32) -> f32 {
        // Logarithmic sweep from 20Hz to 20kHz over 3 seconds
        const SWEEP_DURATION_SECS: f32 = 3.0;
        const MIN_FREQ: f32 = 20.0;
        const MAX_FREQ: f32 = 20000.0;

        // Calculate instantaneous frequency using log sweep
        let t = self.sweep_phase;
        let log_ratio = (MAX_FREQ / MIN_FREQ).ln();
        let freq = MIN_FREQ * (log_ratio * t).exp();

        // Generate sample at current frequency
        let phase_inc = freq / sample_rate as f32;
        let sample = (self.phase * 2.0 * PI).sin();
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Advance sweep phase
        self.sweep_phase += 1.0 / (sample_rate as f32 * SWEEP_DURATION_SECS);
        if self.sweep_phase >= 1.0 {
            self.sweep_phase = 0.0;
        }

        sample
    }

    #[inline]
    fn generate_click(&mut self, sample_rate: u32) -> f32 {
        // Generate a click every 1 second
        const CLICK_INTERVAL_SECS: f32 = 1.0;

        if self.click_counter == 0 {
            self.click_counter = (sample_rate as f32 * CLICK_INTERVAL_SECS) as u32;
            1.0 // Full-scale impulse
        } else {
            self.click_counter -= 1;
            0.0
        }
    }

    /// Generate white noise sample.
    /// Note: Also inlined in generate_pink_noise for borrow checker reasons.
    #[inline]
    #[allow(dead_code)]
    fn white_noise(&mut self) -> f32 {
        // Simple xorshift PRNG
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 17;
        self.random_state ^= self.random_state << 5;

        // Convert to -1.0..1.0 range
        (self.random_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Reset all generator state.
    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.pink_state = PinkNoiseState::default();
        self.sweep_phase = 0.0;
        self.click_counter = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_type_from_u8() {
        assert_eq!(WaveformType::from_u8(0), Some(WaveformType::Sine));
        assert_eq!(WaveformType::from_u8(1), Some(WaveformType::PinkNoise));
        assert_eq!(WaveformType::from_u8(2), Some(WaveformType::ChannelId));
        assert_eq!(WaveformType::from_u8(3), Some(WaveformType::Sweep));
        assert_eq!(WaveformType::from_u8(4), Some(WaveformType::Click));
        assert_eq!(WaveformType::from_u8(5), None);
    }

    #[test]
    fn config_default_values() {
        let config = WaveGeneratorConfig::default();
        assert!(!config.is_enabled());
        assert_eq!(config.waveform(), WaveformType::Sine);
        assert_eq!(config.frequency(), 1000);
        assert!((config.level_db() - DEFAULT_LEVEL_DBFS).abs() < 0.01);
    }

    #[test]
    fn config_level_clamping() {
        let config = WaveGeneratorConfig::default();

        // 0.0 dBFS is too loud - should clamp to MAX_LEVEL_DBFS (-6.0)
        config.set_level_db(0.0);
        assert!((config.level_db() - MAX_LEVEL_DBFS).abs() < 0.01);

        // -3.0 dBFS is too loud - should clamp to MAX_LEVEL_DBFS (-6.0)
        config.set_level_db(-3.0);
        assert!((config.level_db() - MAX_LEVEL_DBFS).abs() < 0.01);

        // -6.0 dBFS is exactly at limit
        config.set_level_db(-6.0);
        assert!((config.level_db() - MAX_LEVEL_DBFS).abs() < 0.01);

        // -20.0 dBFS is quieter than limit - should not be clamped
        config.set_level_db(-20.0);
        assert!((config.level_db() - (-20.0)).abs() < 0.01);
    }

    #[test]
    fn config_linear_gain() {
        let config = WaveGeneratorConfig::default();

        // Even if we request 0 dBFS, it clamps to -6 dBFS (0.5 linear)
        config.set_level_db(0.0);
        assert!((config.linear_gain() - 0.5).abs() < 0.01);

        // -6 dBFS = 0.5 linear
        config.set_level_db(-6.0);
        assert!((config.linear_gain() - 0.5).abs() < 0.01);

        // -20 dBFS = 0.1 linear
        config.set_level_db(-20.0);
        assert!((config.linear_gain() - 0.1).abs() < 0.01);
    }

    #[test]
    fn sine_generation() {
        let config = WaveGeneratorConfig::new(WaveformType::Sine, 1000, -6.0);
        config.set_enabled(true);

        let mut state = WaveGeneratorState::new();
        let sample_rate = 48000;

        // Generate some samples
        let mut samples = Vec::with_capacity(48);
        for _ in 0..48 {
            samples.push(state.generate_sample(&config, sample_rate, 1));
        }

        // Verify samples are in valid range
        for sample in &samples {
            assert!(*sample >= -1.0 && *sample <= 1.0);
        }

        // Verify not all samples are zero
        let sum: f32 = samples.iter().map(|s| s.abs()).sum();
        assert!(sum > 0.0);
    }

    #[test]
    fn disabled_generator_returns_zero() {
        let config = WaveGeneratorConfig::default();
        assert!(!config.is_enabled());

        let mut state = WaveGeneratorState::new();
        for _ in 0..100 {
            let sample = state.generate_sample(&config, 48000, 1);
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn channel_id_different_frequencies() {
        let config = WaveGeneratorConfig::new(WaveformType::ChannelId, 0, -6.0);
        config.set_enabled(true);

        let sample_rate = 48000;

        // Generate samples for two different channels
        let mut state1 = WaveGeneratorState::for_channel(1);
        let mut state2 = WaveGeneratorState::for_channel(2);

        let mut samples1 = Vec::with_capacity(480);
        let mut samples2 = Vec::with_capacity(480);

        for _ in 0..480 {
            samples1.push(state1.generate_sample(&config, sample_rate, 1));
            samples2.push(state2.generate_sample(&config, sample_rate, 2));
        }

        // The samples should be different due to different frequencies
        let mut same_count = 0;
        for (s1, s2) in samples1.iter().zip(samples2.iter()) {
            if (s1 - s2).abs() < 0.001 {
                same_count += 1;
            }
        }
        // Most samples should differ
        assert!(same_count < samples1.len() / 2);
    }

    #[test]
    fn pink_noise_generation() {
        let config = WaveGeneratorConfig::new(WaveformType::PinkNoise, 0, -12.0);
        config.set_enabled(true);

        let mut state = WaveGeneratorState::new();
        let mut samples = Vec::with_capacity(1000);

        for _ in 0..1000 {
            samples.push(state.generate_sample(&config, 48000, 1));
        }

        // Verify samples vary (not all same)
        let first = samples[0];
        let all_same = samples.iter().all(|s| (*s - first).abs() < 0.0001);
        assert!(!all_same);

        // Verify samples are in reasonable range
        for sample in &samples {
            assert!(sample.abs() <= 1.0);
        }
    }

    #[test]
    fn click_generation() {
        let config = WaveGeneratorConfig::new(WaveformType::Click, 0, -6.0);
        config.set_enabled(true);

        let sample_rate = 48000;
        let mut state = WaveGeneratorState::new();

        let mut click_count = 0;
        let mut zero_count = 0;

        // Generate 2 seconds worth of samples
        for _ in 0..(sample_rate * 2) {
            let sample = state.generate_sample(&config, sample_rate, 1);
            if sample.abs() > 0.1 {
                click_count += 1;
            } else {
                zero_count += 1;
            }
        }

        // Should have exactly 2 clicks in 2 seconds
        assert_eq!(click_count, 2);
        assert!(zero_count > sample_rate as usize);
    }

    #[test]
    fn sweep_cycles() {
        let config = WaveGeneratorConfig::new(WaveformType::Sweep, 0, -12.0);
        config.set_enabled(true);

        let sample_rate = 48000;
        let mut state = WaveGeneratorState::new();

        // Generate samples for a partial sweep
        for _ in 0..48000 {
            let sample = state.generate_sample(&config, sample_rate, 1);
            assert!(sample.abs() <= 1.0);
        }

        // Verify sweep phase has advanced
        assert!(state.sweep_phase > 0.0 && state.sweep_phase < 1.0);
    }

    #[test]
    fn block_generation() {
        let config = WaveGeneratorConfig::new(WaveformType::Sine, 1000, -12.0);
        config.set_enabled(true);

        let mut state = WaveGeneratorState::new();
        let mut buffer = [0.0f32; 256];

        state.generate_block(&config, 48000, 1, &mut buffer);

        // Verify buffer was filled
        let sum: f32 = buffer.iter().map(|s| s.abs()).sum();
        assert!(sum > 0.0);
    }

    #[test]
    fn mix_into_block() {
        let config = WaveGeneratorConfig::new(WaveformType::Sine, 1000, -12.0);
        config.set_enabled(true);

        let mut state = WaveGeneratorState::new();
        let mut buffer = [0.5f32; 256]; // Pre-fill with 0.5

        state.mix_into_block(&config, 48000, 1, &mut buffer);

        // Verify samples were added (not replaced)
        // With sine wave mixed in, values should vary from original 0.5
        let all_same = buffer.iter().all(|s| (*s - 0.5).abs() < 0.0001);
        assert!(!all_same);
    }

    #[test]
    fn reset_state() {
        let config = WaveGeneratorConfig::new(WaveformType::Sine, 1000, -12.0);
        config.set_enabled(true);

        let mut state = WaveGeneratorState::new();

        // Advance state
        for _ in 0..1000 {
            state.generate_sample(&config, 48000, 1);
        }

        assert!(state.phase > 0.0);

        // Reset
        state.reset();
        assert_eq!(state.phase, 0.0);
        assert_eq!(state.sweep_phase, 0.0);
        assert_eq!(state.click_counter, 0);
    }
}
