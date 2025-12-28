//! Latency measurement for audio processing.
//!
//! This module provides tools for measuring and tracking audio latency:
//! - Buffer-based latency calculation
//! - Callback timing statistics
//! - Jitter tracking
//!
//! # Latency Sources
//!
//! Total latency = Device buffer + Ring buffer + Processing + (Network if remote)
//!
//! ```text
//! ┌─────────┐   ┌──────────┐   ┌───────────┐   ┌─────────┐
//! │ Input   │ → │ Ring     │ → │ Routing   │ → │ Output  │
//! │ Buffer  │   │ Buffer   │   │ Processing│   │ Buffer  │
//! │ ~5.3ms  │   │ ~42.7ms  │   │ <0.1ms    │   │ ~5.3ms  │
//! └─────────┘   └──────────┘   └───────────┘   └─────────┘
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Calculates latency in samples and milliseconds.
#[derive(Debug, Clone)]
pub struct LatencyCalculator {
    /// Sample rate in Hz.
    sample_rate: u32,
}

impl LatencyCalculator {
    /// Creates a new latency calculator.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Converts samples to duration.
    #[must_use]
    pub fn samples_to_duration(&self, samples: u32) -> Duration {
        let micros = (samples as u64 * 1_000_000) / self.sample_rate as u64;
        Duration::from_micros(micros)
    }

    /// Converts duration to samples.
    #[must_use]
    pub fn duration_to_samples(&self, duration: Duration) -> u32 {
        let micros = duration.as_micros() as u64;
        ((micros * self.sample_rate as u64) / 1_000_000) as u32
    }

    /// Calculates buffer latency.
    #[must_use]
    pub fn buffer_latency_ms(&self, buffer_size: u32) -> f32 {
        (buffer_size as f32 / self.sample_rate as f32) * 1000.0
    }

    /// Calculates total local routing latency.
    ///
    /// Includes:
    /// - Input device buffer
    /// - Ring buffer (worst case: full buffer)
    /// - Output device buffer
    #[must_use]
    pub fn local_latency(
        &self,
        input_buffer: u32,
        ring_buffer: u32,
        output_buffer: u32,
    ) -> LatencyReport {
        let input_ms = self.buffer_latency_ms(input_buffer);
        let ring_ms = self.buffer_latency_ms(ring_buffer);
        let output_ms = self.buffer_latency_ms(output_buffer);
        let total_ms = input_ms + ring_ms + output_ms;

        LatencyReport {
            input_buffer_ms: input_ms,
            ring_buffer_ms: ring_ms,
            output_buffer_ms: output_ms,
            network_ms: 0.0,
            processing_ms: 0.1, // Estimated processing overhead
            total_ms,
            total_samples: input_buffer + ring_buffer + output_buffer,
        }
    }

    /// Calculates network routing latency.
    ///
    /// Adds jitter buffer and network transit time.
    #[must_use]
    pub fn network_latency(
        &self,
        input_buffer: u32,
        ring_buffer: u32,
        jitter_buffer: u32,
        network_rtt_ms: f32,
        output_buffer: u32,
    ) -> LatencyReport {
        let input_ms = self.buffer_latency_ms(input_buffer);
        let ring_ms = self.buffer_latency_ms(ring_buffer);
        let jitter_ms = self.buffer_latency_ms(jitter_buffer);
        let output_ms = self.buffer_latency_ms(output_buffer);
        let network_ms = network_rtt_ms / 2.0 + jitter_ms; // One-way + jitter buffer
        let total_ms = input_ms + ring_ms + network_ms + output_ms + 0.1;

        LatencyReport {
            input_buffer_ms: input_ms,
            ring_buffer_ms: ring_ms,
            output_buffer_ms: output_ms,
            network_ms,
            processing_ms: 0.1,
            total_ms,
            total_samples: input_buffer + ring_buffer + jitter_buffer + output_buffer,
        }
    }
}

impl Default for LatencyCalculator {
    fn default() -> Self {
        Self::new(48000)
    }
}

/// Latency breakdown report.
#[derive(Debug, Clone, Default)]
pub struct LatencyReport {
    /// Input buffer latency in ms.
    pub input_buffer_ms: f32,
    /// Ring buffer latency in ms.
    pub ring_buffer_ms: f32,
    /// Output buffer latency in ms.
    pub output_buffer_ms: f32,
    /// Network latency in ms (one-way + jitter buffer).
    pub network_ms: f32,
    /// Processing overhead in ms.
    pub processing_ms: f32,
    /// Total latency in ms.
    pub total_ms: f32,
    /// Total latency in samples.
    pub total_samples: u32,
}

impl LatencyReport {
    /// Returns true if this is a local route (no network latency).
    #[must_use]
    pub fn is_local(&self) -> bool {
        self.network_ms == 0.0
    }

    /// Formats the latency report as a string.
    #[must_use]
    pub fn format(&self) -> String {
        if self.is_local() {
            format!(
                "Total: {:.2}ms (in:{:.2} + ring:{:.2} + out:{:.2} + proc:{:.2})",
                self.total_ms,
                self.input_buffer_ms,
                self.ring_buffer_ms,
                self.output_buffer_ms,
                self.processing_ms
            )
        } else {
            format!(
                "Total: {:.2}ms (in:{:.2} + ring:{:.2} + net:{:.2} + out:{:.2} + proc:{:.2})",
                self.total_ms,
                self.input_buffer_ms,
                self.ring_buffer_ms,
                self.network_ms,
                self.output_buffer_ms,
                self.processing_ms
            )
        }
    }
}

/// Tracks callback timing for jitter analysis.
///
/// Uses lock-free atomics for thread-safe measurement from audio callbacks.
#[derive(Debug)]
pub struct CallbackTimer {
    /// Timestamp of first callback (for relative timing).
    first_callback_ns: AtomicU64,
    /// Timestamp of last callback.
    last_callback_ns: AtomicU64,
    /// Count of callbacks.
    callback_count: AtomicU64,
    /// Sum of intervals (for average calculation).
    interval_sum_ns: AtomicU64,
    /// Minimum interval observed.
    min_interval_ns: AtomicU64,
    /// Maximum interval observed.
    max_interval_ns: AtomicU64,
    /// Expected interval based on buffer size and sample rate.
    expected_interval_ns: u64,
}

impl CallbackTimer {
    /// Creates a new callback timer.
    ///
    /// # Arguments
    ///
    /// * `buffer_size` - Number of samples per callback
    /// * `sample_rate` - Sample rate in Hz
    #[must_use]
    pub fn new(buffer_size: u32, sample_rate: u32) -> Self {
        let expected_interval_ns = (buffer_size as u64 * 1_000_000_000) / sample_rate as u64;

        Self {
            first_callback_ns: AtomicU64::new(0),
            last_callback_ns: AtomicU64::new(0),
            callback_count: AtomicU64::new(0),
            interval_sum_ns: AtomicU64::new(0),
            min_interval_ns: AtomicU64::new(u64::MAX),
            max_interval_ns: AtomicU64::new(0),
            expected_interval_ns,
        }
    }

    /// Records a callback invocation.
    ///
    /// Call this at the start of each audio callback.
    /// Uses a monotonic timestamp for accurate timing.
    pub fn record(&self) {
        let now_ns = Self::now_ns();

        // Set first callback time if not set
        self.first_callback_ns
            .compare_exchange(0, now_ns, Ordering::SeqCst, Ordering::Relaxed)
            .ok();

        // Calculate interval from last callback
        let last = self.last_callback_ns.swap(now_ns, Ordering::SeqCst);
        if last > 0 {
            let interval = now_ns.saturating_sub(last);
            self.interval_sum_ns.fetch_add(interval, Ordering::Relaxed);

            // Update min (using CAS loop)
            loop {
                let current_min = self.min_interval_ns.load(Ordering::Relaxed);
                if interval >= current_min {
                    break;
                }
                if self
                    .min_interval_ns
                    .compare_exchange_weak(
                        current_min,
                        interval,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    break;
                }
            }

            // Update max (using CAS loop)
            loop {
                let current_max = self.max_interval_ns.load(Ordering::Relaxed);
                if interval <= current_max {
                    break;
                }
                if self
                    .max_interval_ns
                    .compare_exchange_weak(
                        current_max,
                        interval,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    break;
                }
            }
        }

        self.callback_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Gets timing statistics.
    #[must_use]
    pub fn stats(&self) -> TimingStats {
        let count = self.callback_count.load(Ordering::Relaxed);
        let min_ns = self.min_interval_ns.load(Ordering::Relaxed);
        let max_ns = self.max_interval_ns.load(Ordering::Relaxed);
        let sum_ns = self.interval_sum_ns.load(Ordering::Relaxed);

        let avg_ns = if count > 1 {
            sum_ns / (count - 1)
        } else {
            self.expected_interval_ns
        };

        let jitter_ns = if min_ns < u64::MAX {
            max_ns.saturating_sub(min_ns)
        } else {
            0
        };

        TimingStats {
            callback_count: count,
            expected_interval_us: (self.expected_interval_ns / 1000) as u32,
            avg_interval_us: (avg_ns / 1000) as u32,
            min_interval_us: if min_ns < u64::MAX {
                (min_ns / 1000) as u32
            } else {
                0
            },
            max_interval_us: (max_ns / 1000) as u32,
            jitter_us: (jitter_ns / 1000) as u32,
        }
    }

    /// Gets current timestamp in nanoseconds (monotonic).
    fn now_ns() -> u64 {
        // Note: Instant::now() may involve a syscall on some platforms.
        // For truly lock-free operation, consider using TSC on x86 or
        // a pre-calculated offset from a baseline Instant.
        // For now, this is acceptable as it's still very fast.
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let start = START.get_or_init(Instant::now);
        start.elapsed().as_nanos() as u64
    }

    /// Resets all statistics.
    pub fn reset(&self) {
        self.first_callback_ns.store(0, Ordering::SeqCst);
        self.last_callback_ns.store(0, Ordering::SeqCst);
        self.callback_count.store(0, Ordering::Relaxed);
        self.interval_sum_ns.store(0, Ordering::Relaxed);
        self.min_interval_ns.store(u64::MAX, Ordering::Relaxed);
        self.max_interval_ns.store(0, Ordering::Relaxed);
    }
}

/// Timing statistics from callback timer.
#[derive(Debug, Clone, Default)]
pub struct TimingStats {
    /// Number of callbacks processed.
    pub callback_count: u64,
    /// Expected callback interval in microseconds.
    pub expected_interval_us: u32,
    /// Average callback interval in microseconds.
    pub avg_interval_us: u32,
    /// Minimum callback interval in microseconds.
    pub min_interval_us: u32,
    /// Maximum callback interval in microseconds.
    pub max_interval_us: u32,
    /// Jitter (max - min) in microseconds.
    pub jitter_us: u32,
}

impl TimingStats {
    /// Returns the jitter as a percentage of expected interval.
    #[must_use]
    pub fn jitter_percent(&self) -> f32 {
        if self.expected_interval_us > 0 {
            (self.jitter_us as f32 / self.expected_interval_us as f32) * 100.0
        } else {
            0.0
        }
    }

    /// Returns true if timing is stable (jitter < 10% of expected).
    #[must_use]
    pub fn is_stable(&self) -> bool {
        self.jitter_percent() < 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_calculator_samples_to_duration() {
        let calc = LatencyCalculator::new(48000);

        // 48 samples @ 48kHz = 1ms
        let duration = calc.samples_to_duration(48);
        assert_eq!(duration.as_micros(), 1000);

        // 256 samples @ 48kHz = 5.33ms
        let duration = calc.samples_to_duration(256);
        assert!(duration.as_micros() >= 5333 && duration.as_micros() <= 5334);
    }

    #[test]
    fn latency_calculator_buffer_latency() {
        let calc = LatencyCalculator::new(48000);

        // 256 samples @ 48kHz
        let latency = calc.buffer_latency_ms(256);
        assert!((latency - 5.33).abs() < 0.1);

        // 2048 samples @ 48kHz
        let latency = calc.buffer_latency_ms(2048);
        assert!((latency - 42.67).abs() < 0.1);
    }

    #[test]
    fn latency_calculator_local_latency() {
        let calc = LatencyCalculator::new(48000);

        // Typical local route: 256 + 2048 + 256 samples
        let report = calc.local_latency(256, 2048, 256);

        assert!(report.is_local());
        assert!((report.input_buffer_ms - 5.33).abs() < 0.1);
        assert!((report.ring_buffer_ms - 42.67).abs() < 0.1);
        assert!((report.output_buffer_ms - 5.33).abs() < 0.1);
        assert!(report.total_ms > 53.0 && report.total_ms < 54.0);
    }

    #[test]
    fn latency_calculator_network_latency() {
        let calc = LatencyCalculator::new(48000);

        // Network route with 1ms RTT and 512 sample jitter buffer
        let report = calc.network_latency(256, 2048, 512, 1.0, 256);

        assert!(!report.is_local());
        assert!(report.network_ms > 10.0); // Jitter buffer + half RTT
    }

    #[test]
    fn callback_timer_basic() {
        let timer = CallbackTimer::new(256, 48000);

        // Record some callbacks
        timer.record();
        std::thread::sleep(Duration::from_micros(100));
        timer.record();
        std::thread::sleep(Duration::from_micros(100));
        timer.record();

        let stats = timer.stats();
        assert_eq!(stats.callback_count, 3);
        assert!(stats.avg_interval_us > 0);
    }

    #[test]
    fn timing_stats_jitter() {
        let stats = TimingStats {
            callback_count: 100,
            expected_interval_us: 5333, // 256 samples @ 48kHz
            avg_interval_us: 5400,
            min_interval_us: 5000,
            max_interval_us: 6000,
            jitter_us: 1000,
        };

        let jitter_pct = stats.jitter_percent();
        assert!(jitter_pct > 18.0 && jitter_pct < 19.0); // ~18.75%
        assert!(!stats.is_stable()); // > 10%
    }

    #[test]
    fn latency_report_format() {
        let calc = LatencyCalculator::new(48000);
        let report = calc.local_latency(256, 2048, 256);

        let formatted = report.format();
        assert!(formatted.contains("Total:"));
        assert!(formatted.contains("ms"));
    }
}
