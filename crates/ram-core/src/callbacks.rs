//! Lock-free audio callbacks for real-time audio processing.
//!
//! This module provides callback factories that create closures suitable for use
//! with cpal audio streams. All callbacks are designed to be lock-free with no
//! heap allocations in the audio path.
//!
//! # Architecture
//!
//! ```text
//! INPUT DEVICE CALLBACK          OUTPUT DEVICE CALLBACK
//!         │                              ▲
//!         ▼                              │
//!    RingBuffer (SPSC)              RoutingSnapshot
//!         │                              │
//!         └──────► Buffer Pool ◄─────────┘
//!                      │
//!                  RoutingTable (RCU)
//! ```
//!
//! # Lock-Free Guarantees
//!
//! - Input callbacks: Write to ring buffers using atomic operations only
//! - Output callbacks: Read routing snapshot (single atomic load), read from
//!   ring buffers (atomic operations), apply gain (pure math)
//! - No mutex acquisition, no heap allocation, no system calls

use crate::destination::HeadroomMode;
use crate::metering::MeterBank;
use crate::ring_buffer_pool::RingBufferPool;
use crate::routing_snapshot::{DestinationSnapshot, RoutingSnapshot};
use crate::routing_table::RoutingTable;
use crate::Sample;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Maximum number of channels that can be processed in a single callback.
/// This is used for stack-allocated buffers in the callback.
pub const MAX_CALLBACK_CHANNELS: usize = 64;

/// Maximum buffer size in samples per channel.
/// Used to pre-allocate stack buffers for de-interleaving.
pub const MAX_BUFFER_SIZE: usize = 4096;

/// Input callback context containing all state needed for processing.
///
/// This is captured by the input callback closure and provides lock-free
/// access to ring buffers for writing audio samples.
#[derive(Debug)]
pub struct InputCallbackContext {
    /// Buffer pool for writing samples
    buffer_pool: Arc<RingBufferPool>,
    /// Buffer indices for each channel (indexed by channel number)
    buffer_indices: Vec<usize>,
    /// Number of channels being captured
    channel_count: usize,
    /// Device ID for logging/debugging
    device_id: String,
    /// Meter bank for level metering
    meters: MeterBank,
}

impl InputCallbackContext {
    /// Creates a new input callback context.
    ///
    /// # Arguments
    ///
    /// * `buffer_pool` - The ring buffer pool for writing samples
    /// * `buffer_indices` - Buffer index for each channel
    /// * `device_id` - Device identifier for debugging
    #[must_use]
    pub fn new(
        buffer_pool: Arc<RingBufferPool>,
        buffer_indices: Vec<usize>,
        device_id: impl Into<String>,
    ) -> Self {
        let channel_count = buffer_indices.len();
        let meters = MeterBank::new(channel_count);
        Self {
            buffer_pool,
            buffer_indices,
            channel_count,
            device_id: device_id.into(),
            meters,
        }
    }

    /// Returns the device ID.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channel_count
    }

    /// Returns the buffer indices.
    #[must_use]
    pub fn buffer_indices(&self) -> &[usize] {
        &self.buffer_indices
    }

    /// Returns the meter bank for reading levels.
    #[must_use]
    pub fn meters(&self) -> &MeterBank {
        &self.meters
    }
}

/// Output callback context containing all state needed for processing.
///
/// This is captured by the output callback closure and provides lock-free
/// access to the routing table and buffer pool.
#[derive(Debug)]
pub struct OutputCallbackContext {
    /// Routing table for reading current configuration
    routing_table: Arc<RoutingTable>,
    /// Buffer pool for reading source samples
    buffer_pool: Arc<RingBufferPool>,
    /// Destination indices to process (indices into the routing snapshot)
    dest_indices: Vec<usize>,
    /// Cached routing snapshot and its generation
    cached_snapshot: parking_lot::Mutex<CachedSnapshot>,
    /// Device ID for logging/debugging
    device_id: String,
    /// Meter bank for level metering
    meters: MeterBank,
}

/// Cached routing snapshot to avoid repeated Arc clones.
struct CachedSnapshot {
    snapshot: Arc<RoutingSnapshot>,
    generation: u64,
}

impl std::fmt::Debug for CachedSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedSnapshot")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl OutputCallbackContext {
    /// Creates a new output callback context.
    ///
    /// # Arguments
    ///
    /// * `routing_table` - The routing table for reading configuration
    /// * `buffer_pool` - The ring buffer pool for reading samples
    /// * `dest_indices` - Indices of destinations to process
    /// * `device_id` - Device identifier for debugging
    #[must_use]
    pub fn new(
        routing_table: Arc<RoutingTable>,
        buffer_pool: Arc<RingBufferPool>,
        dest_indices: Vec<usize>,
        device_id: impl Into<String>,
    ) -> Self {
        let snapshot = routing_table.snapshot();
        let generation = snapshot.generation;
        let channel_count = dest_indices.len();
        let meters = MeterBank::new(channel_count);
        Self {
            routing_table,
            buffer_pool,
            dest_indices,
            cached_snapshot: parking_lot::Mutex::new(CachedSnapshot {
                snapshot,
                generation,
            }),
            device_id: device_id.into(),
            meters,
        }
    }

    /// Returns the device ID.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the destination indices.
    #[must_use]
    pub fn dest_indices(&self) -> &[usize] {
        &self.dest_indices
    }

    /// Returns the meter bank for reading levels.
    #[must_use]
    pub fn meters(&self) -> &MeterBank {
        &self.meters
    }
}

/// Creates an input callback that de-interleaves and writes to ring buffers.
///
/// The returned closure is suitable for use with cpal's `build_input_stream`.
/// It performs the following operations (all lock-free):
///
/// 1. De-interleaves the input buffer into per-channel samples
/// 2. Writes each channel's samples to its corresponding ring buffer
///
/// # Arguments
///
/// * `context` - The input callback context
///
/// # Returns
///
/// A closure that processes input audio samples.
///
/// # Lock-Free Guarantees
///
/// This callback performs no locking, no heap allocation, and no system calls.
/// It is safe to call from a real-time audio thread.
pub fn create_input_callback(
    context: Arc<InputCallbackContext>,
) -> impl FnMut(&[Sample]) + Send + 'static {
    let mut callback_count = 0u64;
    move |data: &[Sample]| {
        let channels = context.channel_count;
        if channels == 0 {
            return;
        }

        let samples_per_channel = data.len() / channels;
        if samples_per_channel == 0 {
            return;
        }

        callback_count += 1;

        // Debug: log every ~1000 callbacks (~30 seconds at 30Hz)
        if callback_count % 1000 == 1 {
            // Calculate max sample in buffer
            let max_sample = data.iter().fold(0.0f32, |max, &s| s.abs().max(max));
            tracing::debug!(
                "Input callback #{} for {}: {} samples, {} channels, max sample: {:.4}",
                callback_count,
                context.device_id,
                data.len(),
                channels,
                max_sample
            );
        }

        // Update meters with interleaved data
        context.meters.update_interleaved(data);

        // De-interleave and write to ring buffers
        // Stack-allocated buffer for de-interleaving (one channel at a time)
        let mut channel_buffer = [0.0f32; MAX_BUFFER_SIZE];
        let samples_to_process = samples_per_channel.min(MAX_BUFFER_SIZE);

        for (ch, &buffer_idx) in context.buffer_indices.iter().enumerate() {
            if let Some(ring_buffer) = context.buffer_pool.get(buffer_idx) {
                // De-interleave this channel
                for (i, sample) in channel_buffer
                    .iter_mut()
                    .take(samples_to_process)
                    .enumerate()
                {
                    *sample = data[i * channels + ch];
                }

                // Write to ring buffer
                ring_buffer.write(&channel_buffer[..samples_to_process]);
            }
        }
    }
}

/// Creates an output callback that mixes sources and writes to output buffer.
///
/// The returned closure is suitable for use with cpal's `build_output_stream`.
/// It performs the following operations (all lock-free):
///
/// 1. Gets the current routing snapshot (single atomic load + possible Arc clone)
/// 2. For each destination channel:
///    a. Reads samples from all source ring buffers
///    b. Applies per-source gain
///    c. Mixes samples together
///    d. Applies headroom mode (clip/autogain/limiter)
/// 3. Interleaves the output
///
/// # Arguments
///
/// * `context` - The output callback context
///
/// # Returns
///
/// A closure that fills output audio buffers.
///
/// # Lock-Free Guarantees
///
/// The only lock acquired is on the cached snapshot, which is held briefly.
/// All audio processing uses atomic operations and pure math only.
#[allow(clippy::too_many_lines)]
pub fn create_output_callback(
    context: Arc<OutputCallbackContext>,
) -> impl FnMut(&mut [Sample]) + Send + 'static {
    move |data: &mut [Sample]| {
        let channels = context.dest_indices.len();
        if channels == 0 {
            // Zero the output
            data.fill(0.0);
            return;
        }

        let samples_per_channel = data.len() / channels;
        if samples_per_channel == 0 {
            return;
        }

        // Check if routing has changed and update cached snapshot if needed
        let snapshot = {
            let current_gen = context.routing_table.generation();
            let mut cached = context.cached_snapshot.lock();
            if cached.generation != current_gen {
                cached.snapshot = context.routing_table.snapshot();
                cached.generation = cached.snapshot.generation;
            }
            Arc::clone(&cached.snapshot)
        };

        // Stack-allocated buffers for mixing
        let mut mix_buffer = [0.0f32; MAX_BUFFER_SIZE];
        let mut source_buffer = [0.0f32; MAX_BUFFER_SIZE];
        let samples_to_process = samples_per_channel.min(MAX_BUFFER_SIZE);

        // Process each destination channel
        for (ch_out, &dest_idx) in context.dest_indices.iter().enumerate() {
            // Clear mix buffer
            mix_buffer[..samples_to_process].fill(0.0);

            // Get destination from snapshot
            if let Some(dest) = snapshot.destinations.get(dest_idx) {
                // Mix all active sources
                mix_sources(
                    dest,
                    &context.buffer_pool,
                    &mut mix_buffer[..samples_to_process],
                    &mut source_buffer[..samples_to_process],
                );

                // Apply headroom mode
                apply_headroom(dest, &mut mix_buffer[..samples_to_process]);
            }

            // Update meter for this channel (after mixing, before final output)
            context
                .meters
                .update_channel(ch_out, &mix_buffer[..samples_to_process]);

            // Interleave this channel into output
            for (i, &sample) in mix_buffer.iter().take(samples_to_process).enumerate() {
                data[i * channels + ch_out] = sample;
            }
        }
    }
}

/// Mixes all active sources from a destination into the mix buffer.
///
/// This is a helper function that reads from ring buffers and applies gain.
/// All operations are lock-free.
#[inline]
fn mix_sources(
    dest: &DestinationSnapshot,
    buffer_pool: &RingBufferPool,
    mix_buffer: &mut [Sample],
    source_buffer: &mut [Sample],
) {
    for source_slot in dest.sources.iter().flatten() {
        // Get effective gain (0.0 if muted or disabled)
        let gain = source_slot.effective_gain();
        if gain.abs() < f32::EPSILON {
            continue;
        }

        // Read from ring buffer
        if let Some(ring_buffer) = buffer_pool.get(source_slot.ring_buffer_index) {
            let read = ring_buffer.read(source_buffer);

            // Mix with gain
            for i in 0..read {
                mix_buffer[i] += source_buffer[i] * gain;
            }

            // Zero-fill if ring buffer was short
            // (this is normal during stream startup)
        }
    }
}

/// Applies headroom mode to the mixed samples.
///
/// This is a helper function that handles clipping, auto-gain, limiting, etc.
/// All operations are pure math (lock-free).
#[inline]
fn apply_headroom(dest: &DestinationSnapshot, samples: &mut [Sample]) {
    let mode = dest.headroom_mode.load();
    let manual_headroom_db = dest.manual_headroom_db.load(Ordering::Relaxed);

    match mode {
        HeadroomMode::Clip => {
            apply_clip(samples);
        },
        HeadroomMode::AutoGain => {
            apply_auto_gain(samples);
        },
        HeadroomMode::Limiter => {
            // Simple soft-knee limiter
            apply_soft_limiter(samples);
        },
        HeadroomMode::Manual => {
            // Apply fixed attenuation
            let gain = db_to_linear(manual_headroom_db);
            for sample in samples.iter_mut() {
                *sample *= gain;
            }
            apply_clip(samples);
        },
    }
}

/// Hard clips samples to [-1.0, 1.0].
#[inline]
fn apply_clip(samples: &mut [Sample]) {
    for sample in samples.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
}

/// Auto-gain: scales entire buffer to prevent clipping.
#[inline]
fn apply_auto_gain(samples: &mut [Sample]) {
    // Find peak
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);

    if peak > 1.0 {
        let scale = 1.0 / peak;
        for sample in samples.iter_mut() {
            *sample *= scale;
        }
    }
}

/// Soft-knee limiter using tanh-like curve.
#[inline]
fn apply_soft_limiter(samples: &mut [Sample]) {
    const THRESHOLD: f32 = 0.8;
    const KNEE: f32 = 0.2;

    for sample in samples.iter_mut() {
        let abs_val = sample.abs();
        if abs_val > THRESHOLD {
            // Soft-knee compression above threshold
            let excess = abs_val - THRESHOLD;
            let compressed = THRESHOLD + KNEE * (1.0 - (-excess / KNEE).exp());
            *sample = compressed.copysign(*sample);
        }
    }
}

/// Converts decibels to linear gain.
#[inline]
#[must_use]
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_snapshot::{DestinationSnapshot, RoutingSnapshot};

    #[test]
    fn input_context_creation() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let indices = vec![0, 1, 2];
        let context = InputCallbackContext::new(pool, indices.clone(), "test-device");

        assert_eq!(context.device_id(), "test-device");
        assert_eq!(context.channel_count(), 3);
        assert_eq!(context.buffer_indices(), &indices);
    }

    #[test]
    fn output_context_creation() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let routing = Arc::new(RoutingTable::new());
        let context = OutputCallbackContext::new(routing, pool, vec![0, 1], "test-device");

        assert_eq!(context.device_id(), "test-device");
        assert_eq!(context.dest_indices(), &[0, 1]);
    }

    #[test]
    fn input_callback_de_interleaves() {
        let pool = Arc::new(RingBufferPool::new(16, 256));

        // Allocate buffers for 2 channels
        let idx0 = pool.allocate().unwrap();
        let idx1 = pool.allocate().unwrap();

        let context = Arc::new(InputCallbackContext::new(
            Arc::clone(&pool),
            vec![idx0, idx1],
            "test",
        ));

        let callback = create_input_callback(context);
        let mut callback = callback;

        // Interleaved stereo data: L0, R0, L1, R1, L2, R2, L3, R3
        let input = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        callback(&input);

        // Check left channel buffer
        let left_buffer = pool.get(idx0).unwrap();
        let mut left_out = [0.0f32; 4];
        left_buffer.read(&mut left_out);
        assert!((left_out[0] - 0.1).abs() < 0.001);
        assert!((left_out[1] - 0.3).abs() < 0.001);
        assert!((left_out[2] - 0.5).abs() < 0.001);
        assert!((left_out[3] - 0.7).abs() < 0.001);

        // Check right channel buffer
        let right_buffer = pool.get(idx1).unwrap();
        let mut right_out = [0.0f32; 4];
        right_buffer.read(&mut right_out);
        assert!((right_out[0] - 0.2).abs() < 0.001);
        assert!((right_out[1] - 0.4).abs() < 0.001);
        assert!((right_out[2] - 0.6).abs() < 0.001);
        assert!((right_out[3] - 0.8).abs() < 0.001);
    }

    #[test]
    fn output_callback_zeros_when_empty() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let routing = Arc::new(RoutingTable::new());
        let context = Arc::new(OutputCallbackContext::new(
            routing,
            pool,
            vec![], // No destinations
            "test",
        ));

        let mut callback = create_output_callback(context);
        let mut output = [1.0f32; 8];
        callback(&mut output);

        // Should be zeroed
        assert!(output.iter().all(|&s| s.abs() < 0.001));
    }

    #[test]
    fn apply_clip_limits_samples() {
        let mut samples = [-2.0, -1.0, 0.0, 1.0, 2.0];
        apply_clip(&mut samples);
        assert_eq!(samples, [-1.0, -1.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn apply_auto_gain_scales_when_needed() {
        let mut samples = [0.0, 1.0, 2.0, 1.0, 0.0];
        apply_auto_gain(&mut samples);
        // Peak was 2.0, so scaled by 0.5
        assert!((samples[2] - 1.0).abs() < 0.001);
        assert!((samples[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn apply_auto_gain_no_change_when_not_needed() {
        let mut samples = [0.0, 0.5, 0.8, 0.5, 0.0];
        let original = samples;
        apply_auto_gain(&mut samples);
        // No scaling needed
        for (a, b) in samples.iter().zip(original.iter()) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn apply_soft_limiter_compresses_peaks() {
        let mut samples = [0.5, 0.9, 1.5, 0.7, 0.3];
        apply_soft_limiter(&mut samples);

        // Below threshold should be unchanged
        assert!((samples[0] - 0.5).abs() < 0.001);
        assert!((samples[3] - 0.7).abs() < 0.001);
        assert!((samples[4] - 0.3).abs() < 0.001);

        // Above threshold should be compressed
        assert!(samples[1] < 0.95); // 0.9 slightly compressed
        assert!(samples[2] < 1.5); // 1.5 significantly compressed
        assert!(samples[2] > 0.8); // But still audible
    }

    #[test]
    fn db_to_linear_conversion() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.001);
        assert!((db_to_linear(-6.0) - 0.501).abs() < 0.01);
        assert!((db_to_linear(-20.0) - 0.1).abs() < 0.01);
        assert!((db_to_linear(6.0) - 2.0).abs() < 0.1);
    }

    #[test]
    fn output_callback_mixes_sources() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let routing = Arc::new(RoutingTable::new());

        // Allocate a source buffer and write some samples
        let source_idx = pool.allocate().unwrap();
        let source_buffer = pool.get(source_idx).unwrap();
        let source_samples = [0.5f32; 4];
        source_buffer.write(&source_samples);

        // Create a routing snapshot with one destination and one source
        let mut dest = DestinationSnapshot::new("dest:0".to_string(), "device".to_string(), 0);
        dest.add_source(source_idx);

        let mut snapshot = RoutingSnapshot::new();
        snapshot.destinations.push(dest);
        routing.update(snapshot);

        let context = Arc::new(OutputCallbackContext::new(
            Arc::clone(&routing),
            Arc::clone(&pool),
            vec![0], // Process destination index 0
            "test",
        ));

        let mut callback = create_output_callback(context);
        let mut output = [0.0f32; 4];
        callback(&mut output);

        // Output should be 0.5 (the source samples)
        for sample in &output {
            assert!((sample - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn output_callback_applies_gain() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let routing = Arc::new(RoutingTable::new());

        // Allocate and write source samples
        let source_idx = pool.allocate().unwrap();
        let source_buffer = pool.get(source_idx).unwrap();
        source_buffer.write(&[1.0f32; 4]);

        // Create routing with gain = 0.5
        let mut dest = DestinationSnapshot::new("dest:0".to_string(), "device".to_string(), 0);
        dest.add_source(source_idx);
        // Set gain to 0.5
        if let Some(slot) = dest.find_source(source_idx) {
            slot.gain.set(0.5);
        }

        let mut snapshot = RoutingSnapshot::new();
        snapshot.destinations.push(dest);
        routing.update(snapshot);

        let context = Arc::new(OutputCallbackContext::new(
            Arc::clone(&routing),
            Arc::clone(&pool),
            vec![0],
            "test",
        ));

        let mut callback = create_output_callback(context);
        let mut output = [0.0f32; 4];
        callback(&mut output);

        // Output should be 0.5 (1.0 * 0.5 gain)
        for sample in &output {
            assert!((sample - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn output_callback_respects_mute() {
        let pool = Arc::new(RingBufferPool::new(16, 256));
        let routing = Arc::new(RoutingTable::new());

        // Allocate and write source samples
        let source_idx = pool.allocate().unwrap();
        let source_buffer = pool.get(source_idx).unwrap();
        source_buffer.write(&[1.0f32; 4]);

        // Create routing with muted source
        let mut dest = DestinationSnapshot::new("dest:0".to_string(), "device".to_string(), 0);
        dest.add_source(source_idx);
        if let Some(slot) = dest.find_source(source_idx) {
            slot.muted.store(true, Ordering::Relaxed);
        }

        let mut snapshot = RoutingSnapshot::new();
        snapshot.destinations.push(dest);
        routing.update(snapshot);

        let context = Arc::new(OutputCallbackContext::new(
            Arc::clone(&routing),
            Arc::clone(&pool),
            vec![0],
            "test",
        ));

        let mut callback = create_output_callback(context);
        let mut output = [0.0f32; 4];
        callback(&mut output);

        // Output should be 0 (source is muted)
        for sample in &output {
            assert!(sample.abs() < 0.001);
        }
    }
}
