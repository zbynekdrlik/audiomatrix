//! Lock-free audio level metering for real-time visualization.
//!
//! This module provides thread-safe metering infrastructure that can be safely
//! updated from audio callbacks and read from UI/API threads.
//!
//! # Features
//!
//! - Peak and RMS level measurement
//! - Lock-free updates using atomics
//! - Configurable decay time
//! - Per-channel metering
//!
//! # Example
//!
//! ```
//! use ram_core::metering::ChannelMeter;
//!
//! let meter = ChannelMeter::new();
//!
//! // In audio callback
//! let samples = [0.5f32, 0.8, 0.3, 0.6];
//! meter.update(&samples);
//!
//! // From UI thread
//! let levels = meter.levels();
//! println!("Peak: {:.2} dB, RMS: {:.2} dB", levels.peak_db, levels.rms_db);
//! ```

use crate::atomic::AtomicF32;
use std::sync::atomic::{AtomicU64, Ordering};

/// Metering levels for a single channel.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeterLevels {
    /// Peak level (linear, 0.0 to 1.0+).
    pub peak: f32,
    /// RMS level (linear, 0.0 to 1.0+).
    pub rms: f32,
    /// Peak level in decibels (-inf to 0+).
    pub peak_db: f32,
    /// RMS level in decibels (-inf to 0+).
    pub rms_db: f32,
    /// Whether the channel is clipping (peak > 1.0).
    pub clipping: bool,
}

impl MeterLevels {
    /// Creates levels from linear values.
    #[must_use]
    pub fn from_linear(peak: f32, rms: f32) -> Self {
        Self {
            peak,
            rms,
            peak_db: linear_to_db(peak),
            rms_db: linear_to_db(rms),
            clipping: peak > 1.0,
        }
    }
}

/// Lock-free meter for a single audio channel.
///
/// Uses atomics for thread-safe updates from audio callbacks.
#[derive(Debug)]
pub struct ChannelMeter {
    /// Peak level (linear).
    peak: AtomicF32,
    /// Accumulated squared sum for RMS calculation.
    rms_sum: AtomicF32,
    /// Sample count for RMS calculation.
    sample_count: AtomicU64,
    /// Held peak for decay (highest peak seen recently).
    held_peak: AtomicF32,
}

impl Default for ChannelMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelMeter {
    /// Creates a new channel meter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            peak: AtomicF32::new(0.0),
            rms_sum: AtomicF32::new(0.0),
            sample_count: AtomicU64::new(0),
            held_peak: AtomicF32::new(0.0),
        }
    }

    /// Updates the meter with new samples.
    ///
    /// This is lock-free and safe to call from audio callbacks.
    pub fn update(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        // Calculate peak
        let mut peak = 0.0f32;
        let mut sum_squared = 0.0f32;

        for &sample in samples {
            let abs = sample.abs();
            if abs > peak {
                peak = abs;
            }
            sum_squared += sample * sample;
        }

        // Update peak (keep max)
        let current_peak = self.peak.get();
        if peak > current_peak {
            self.peak.set(peak);
        }

        // Update held peak
        let current_held = self.held_peak.get();
        if peak > current_held {
            self.held_peak.set(peak);
        }

        // Accumulate RMS
        let current_sum = self.rms_sum.get();
        self.rms_sum.set(current_sum + sum_squared);
        self.sample_count.fetch_add(samples.len() as u64, Ordering::Relaxed);
    }

    /// Returns current meter levels.
    ///
    /// This is safe to call from any thread.
    #[must_use]
    pub fn levels(&self) -> MeterLevels {
        let peak = self.peak.get();
        let sample_count = self.sample_count.load(Ordering::Relaxed);

        let rms = if sample_count > 0 {
            let sum = self.rms_sum.get();
            (sum / sample_count as f32).sqrt()
        } else {
            0.0
        };

        MeterLevels::from_linear(peak, rms)
    }

    /// Returns the held peak level (highest peak since last reset).
    #[must_use]
    pub fn held_peak(&self) -> f32 {
        self.held_peak.get()
    }

    /// Resets the meter.
    ///
    /// This should be called periodically (e.g., every 50ms) to allow
    /// levels to decay.
    pub fn reset(&self) {
        self.peak.set(0.0);
        self.rms_sum.set(0.0);
        self.sample_count.store(0, Ordering::Relaxed);
    }

    /// Resets everything including held peak.
    pub fn reset_all(&self) {
        self.reset();
        self.held_peak.set(0.0);
    }

    /// Applies decay to the peak value.
    ///
    /// Call this periodically for smooth meter decay.
    /// The decay_factor should be between 0.0 (instant decay) and 1.0 (no decay).
    /// Typical values are 0.9 to 0.99.
    pub fn apply_decay(&self, decay_factor: f32) {
        let current = self.peak.get();
        self.peak.set(current * decay_factor);
    }
}

/// Collection of channel meters for multi-channel audio.
#[derive(Debug)]
pub struct MeterBank {
    /// Individual channel meters.
    channels: Vec<ChannelMeter>,
    /// Number of channels.
    channel_count: usize,
}

impl MeterBank {
    /// Creates a new meter bank with the specified number of channels.
    #[must_use]
    pub fn new(channel_count: usize) -> Self {
        let channels = (0..channel_count).map(|_| ChannelMeter::new()).collect();
        Self {
            channels,
            channel_count,
        }
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    /// Updates meters from interleaved audio data.
    ///
    /// The data should be interleaved: [ch0_s0, ch1_s0, ch0_s1, ch1_s1, ...]
    pub fn update_interleaved(&self, data: &[f32]) {
        if self.channel_count == 0 {
            return;
        }

        let samples_per_channel = data.len() / self.channel_count;
        if samples_per_channel == 0 {
            return;
        }

        // Process each channel
        for (ch, meter) in self.channels.iter().enumerate() {
            // Calculate peak and RMS for this channel
            let mut peak = 0.0f32;
            let mut sum_squared = 0.0f32;

            for i in 0..samples_per_channel {
                let sample = data[i * self.channel_count + ch];
                let abs = sample.abs();
                if abs > peak {
                    peak = abs;
                }
                sum_squared += sample * sample;
            }

            // Update meter
            let current_peak = meter.peak.get();
            if peak > current_peak {
                meter.peak.set(peak);
            }

            let current_held = meter.held_peak.get();
            if peak > current_held {
                meter.held_peak.set(peak);
            }

            let current_sum = meter.rms_sum.get();
            meter.rms_sum.set(current_sum + sum_squared);
            meter.sample_count.fetch_add(samples_per_channel as u64, Ordering::Relaxed);
        }
    }

    /// Updates a specific channel from de-interleaved data.
    pub fn update_channel(&self, channel: usize, samples: &[f32]) {
        if let Some(meter) = self.channels.get(channel) {
            meter.update(samples);
        }
    }

    /// Returns meter levels for all channels.
    #[must_use]
    pub fn all_levels(&self) -> Vec<MeterLevels> {
        self.channels.iter().map(ChannelMeter::levels).collect()
    }

    /// Returns meter levels for a specific channel.
    #[must_use]
    pub fn channel_levels(&self, channel: usize) -> Option<MeterLevels> {
        self.channels.get(channel).map(ChannelMeter::levels)
    }

    /// Returns the meter for a specific channel.
    #[must_use]
    pub fn get(&self, channel: usize) -> Option<&ChannelMeter> {
        self.channels.get(channel)
    }

    /// Resets all meters.
    pub fn reset(&self) {
        for meter in &self.channels {
            meter.reset();
        }
    }

    /// Resets all meters including held peaks.
    pub fn reset_all(&self) {
        for meter in &self.channels {
            meter.reset_all();
        }
    }

    /// Applies decay to all channels.
    pub fn apply_decay(&self, decay_factor: f32) {
        for meter in &self.channels {
            meter.apply_decay(decay_factor);
        }
    }
}

/// Converts linear level to decibels.
///
/// Returns -120.0 for values at or below 0.0.
#[inline]
#[must_use]
pub fn linear_to_db(linear: f32) -> f32 {
    const MIN_DB: f32 = -120.0;
    if linear <= 0.0 {
        MIN_DB
    } else {
        20.0 * linear.log10()
    }
}

/// Converts decibels to linear level.
#[inline]
#[must_use]
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_meter_new() {
        let meter = ChannelMeter::new();
        let levels = meter.levels();
        assert_eq!(levels.peak, 0.0);
        assert_eq!(levels.rms, 0.0);
    }

    #[test]
    fn channel_meter_update() {
        let meter = ChannelMeter::new();
        meter.update(&[0.5, 0.3, 0.8, 0.1]);

        let levels = meter.levels();
        assert!((levels.peak - 0.8).abs() < 0.001);
        assert!(levels.rms > 0.0);
    }

    #[test]
    fn channel_meter_rms() {
        let meter = ChannelMeter::new();
        // Constant level of 0.5
        meter.update(&[0.5, 0.5, 0.5, 0.5]);

        let levels = meter.levels();
        // RMS of constant should equal the value
        assert!((levels.rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn channel_meter_clipping() {
        let meter = ChannelMeter::new();
        meter.update(&[0.5, 1.5, 0.3]); // 1.5 exceeds 1.0

        let levels = meter.levels();
        assert!(levels.clipping);
        assert!((levels.peak - 1.5).abs() < 0.001);
    }

    #[test]
    fn channel_meter_reset() {
        let meter = ChannelMeter::new();
        meter.update(&[0.5, 0.8]);

        assert!(meter.levels().peak > 0.0);

        meter.reset();

        let levels = meter.levels();
        assert_eq!(levels.peak, 0.0);
    }

    #[test]
    fn channel_meter_held_peak() {
        let meter = ChannelMeter::new();
        meter.update(&[0.8]);
        meter.reset();
        meter.update(&[0.5]);

        // Current peak should be 0.5
        assert!((meter.levels().peak - 0.5).abs() < 0.001);

        // Held peak should still be 0.8
        assert!((meter.held_peak() - 0.8).abs() < 0.001);
    }

    #[test]
    fn channel_meter_decay() {
        let meter = ChannelMeter::new();
        meter.update(&[1.0]);

        meter.apply_decay(0.5);

        assert!((meter.levels().peak - 0.5).abs() < 0.001);
    }

    #[test]
    fn meter_bank_new() {
        let bank = MeterBank::new(4);
        assert_eq!(bank.channel_count(), 4);
    }

    #[test]
    fn meter_bank_update_interleaved() {
        let bank = MeterBank::new(2);

        // Stereo interleaved: L, R, L, R
        // L channel: 0.5, 0.3 -> peak 0.5
        // R channel: 0.8, 0.1 -> peak 0.8
        bank.update_interleaved(&[0.5, 0.8, 0.3, 0.1]);

        let levels = bank.all_levels();
        assert_eq!(levels.len(), 2);
        assert!((levels[0].peak - 0.5).abs() < 0.001);
        assert!((levels[1].peak - 0.8).abs() < 0.001);
    }

    #[test]
    fn meter_bank_update_channel() {
        let bank = MeterBank::new(2);

        bank.update_channel(0, &[0.5]);
        bank.update_channel(1, &[0.8]);

        let levels = bank.all_levels();
        assert!((levels[0].peak - 0.5).abs() < 0.001);
        assert!((levels[1].peak - 0.8).abs() < 0.001);
    }

    #[test]
    fn meter_bank_channel_levels() {
        let bank = MeterBank::new(2);
        bank.update_channel(0, &[0.5]);

        let levels = bank.channel_levels(0);
        assert!(levels.is_some());
        assert!((levels.unwrap().peak - 0.5).abs() < 0.001);

        let invalid = bank.channel_levels(5);
        assert!(invalid.is_none());
    }

    #[test]
    fn meter_bank_reset() {
        let bank = MeterBank::new(2);
        bank.update_channel(0, &[0.5]);
        bank.update_channel(1, &[0.8]);

        bank.reset();

        let levels = bank.all_levels();
        assert_eq!(levels[0].peak, 0.0);
        assert_eq!(levels[1].peak, 0.0);
    }

    #[test]
    fn linear_to_db_conversion() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.001);
        assert!((linear_to_db(0.5) - (-6.02)).abs() < 0.1);
        assert!((linear_to_db(0.1) - (-20.0)).abs() < 0.1);
        assert_eq!(linear_to_db(0.0), -120.0);
    }

    #[test]
    fn db_to_linear_conversion() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-6.0) - 0.501).abs() < 0.01);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.01);
    }

    #[test]
    fn roundtrip_conversion() {
        for db in [-60.0, -40.0, -20.0, -6.0, 0.0, 6.0] {
            let linear = db_to_linear(db);
            let back = linear_to_db(linear);
            assert!((back - db).abs() < 0.01);
        }
    }
}
