//! Lock-free ring buffer for audio samples.

use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam_utils::CachePadded;

use crate::Sample;

/// A lock-free single-producer single-consumer ring buffer.
///
/// This buffer is designed for real-time audio where one thread produces
/// samples and another consumes them without any locking.
///
/// # Cache Line Optimization
///
/// The read and write positions are wrapped in `CachePadded` to prevent
/// false sharing. Without this, the positions could share a cache line,
/// causing cache invalidation whenever either position is updated.
/// This is critical for audio where producer and consumer run on
/// separate threads with tight latency requirements.
pub struct RingBuffer {
    data: Box<[Sample]>,
    capacity: usize,
    /// Reader position - padded to its own cache line
    read_pos: CachePadded<AtomicUsize>,
    /// Writer position - padded to its own cache line
    write_pos: CachePadded<AtomicUsize>,
}

impl RingBuffer {
    /// Creates a new ring buffer with the specified capacity.
    ///
    /// The actual capacity will be rounded up to the next power of two
    /// for efficient modulo operations.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            data: vec![0.0; capacity].into_boxed_slice(),
            capacity,
            read_pos: CachePadded::new(AtomicUsize::new(0)),
            write_pos: CachePadded::new(AtomicUsize::new(0)),
        }
    }

    /// Returns the capacity of the buffer.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of samples available to read.
    #[must_use]
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        write.wrapping_sub(read)
    }

    /// Returns the amount of free space in the buffer.
    #[must_use]
    pub fn free(&self) -> usize {
        self.capacity - self.available()
    }

    /// Writes samples to the buffer.
    ///
    /// Returns the number of samples actually written.
    pub fn write(&self, samples: &[Sample]) -> usize {
        let free = self.free();
        let to_write = samples.len().min(free);

        if to_write == 0 {
            return 0;
        }

        let write_pos = self.write_pos.load(Ordering::Relaxed);
        let mask = self.capacity - 1;

        // SAFETY: We're the only writer and we've checked bounds
        let data_ptr = self.data.as_ptr().cast_mut();

        for (i, &sample) in samples.iter().take(to_write).enumerate() {
            let idx = (write_pos + i) & mask;
            // SAFETY: idx is always within bounds due to masking
            unsafe {
                data_ptr.add(idx).write(sample);
            }
        }

        self.write_pos
            .store(write_pos.wrapping_add(to_write), Ordering::Release);
        to_write
    }

    /// Reads samples from the buffer.
    ///
    /// Returns the number of samples actually read.
    pub fn read(&self, output: &mut [Sample]) -> usize {
        let available = self.available();
        let to_read = output.len().min(available);

        if to_read == 0 {
            return 0;
        }

        let read_pos = self.read_pos.load(Ordering::Relaxed);
        let mask = self.capacity - 1;

        for (i, sample) in output.iter_mut().take(to_read).enumerate() {
            let idx = (read_pos + i) & mask;
            *sample = self.data[idx];
        }

        self.read_pos
            .store(read_pos.wrapping_add(to_read), Ordering::Release);
        to_read
    }

    /// Clears all data from the buffer.
    pub fn clear(&self) {
        self.read_pos.store(0, Ordering::Release);
        self.write_pos.store(0, Ordering::Release);
    }
}

// SAFETY: RingBuffer uses atomic operations for synchronization
unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}

impl std::fmt::Debug for RingBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RingBuffer")
            .field("capacity", &self.capacity)
            .field("available", &self.available())
            .field("free", &self.free())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let buffer = RingBuffer::new(1024);
        assert_eq!(buffer.available(), 0);
        assert!(buffer.free() >= 1024);
    }

    #[test]
    fn write_read_roundtrip() {
        let buffer = RingBuffer::new(1024);
        let samples = [0.5_f32; 64];

        let written = buffer.write(&samples);
        assert_eq!(written, 64);
        assert_eq!(buffer.available(), 64);

        let mut output = [0.0_f32; 64];
        let read = buffer.read(&mut output);
        assert_eq!(read, 64);
        assert!(output
            .iter()
            .zip(samples.iter())
            .all(|(a, b)| (a - b).abs() < f32::EPSILON));
        assert_eq!(buffer.available(), 0);
    }

    #[test]
    fn write_respects_capacity() {
        let buffer = RingBuffer::new(64);
        let samples = [0.5_f32; 128];

        let written = buffer.write(&samples);
        assert!(written <= 64);
    }

    #[test]
    fn clear_resets_buffer() {
        let buffer = RingBuffer::new(1024);
        let samples = [0.5_f32; 64];

        buffer.write(&samples);
        assert_eq!(buffer.available(), 64);

        buffer.clear();
        assert_eq!(buffer.available(), 0);
    }

    #[test]
    fn read_from_empty_returns_zero() {
        let buffer = RingBuffer::new(64);
        let mut output = [1.0_f32; 16];
        let read = buffer.read(&mut output);
        assert_eq!(read, 0);
    }

    #[test]
    fn write_to_full_returns_zero() {
        let buffer = RingBuffer::new(64);
        let samples = [0.5_f32; 64];
        buffer.write(&samples);

        // Buffer should be full now
        let more_samples = [0.5_f32; 16];
        let written = buffer.write(&more_samples);
        assert_eq!(written, 0);
    }

    #[test]
    fn partial_read() {
        let buffer = RingBuffer::new(1024);
        let samples = [0.5_f32; 64];
        buffer.write(&samples);

        // Read less than available
        let mut output = [0.0_f32; 32];
        let read = buffer.read(&mut output);
        assert_eq!(read, 32);
        assert_eq!(buffer.available(), 32);
    }

    #[test]
    fn read_more_than_available() {
        let buffer = RingBuffer::new(1024);
        let samples = [0.5_f32; 32];
        buffer.write(&samples);

        let mut output = [0.0_f32; 64];
        let read = buffer.read(&mut output);
        assert_eq!(read, 32);
    }

    #[test]
    fn wraparound_write_read() {
        let buffer = RingBuffer::new(64);

        // Fill partially
        let samples1 = [0.25_f32; 48];
        buffer.write(&samples1);

        // Read some
        let mut output1 = [0.0_f32; 32];
        buffer.read(&mut output1);

        // Write more (should wrap around)
        let samples2 = [0.75_f32; 32];
        let written = buffer.write(&samples2);
        assert_eq!(written, 32);

        // Read remaining samples1 data
        let mut output2 = [0.0_f32; 16];
        let read = buffer.read(&mut output2);
        assert_eq!(read, 16);
        for sample in &output2 {
            assert!((*sample - 0.25).abs() < 0.001);
        }

        // Read samples2 data
        let mut output3 = [0.0_f32; 32];
        let read = buffer.read(&mut output3);
        assert_eq!(read, 32);
        for sample in &output3 {
            assert!((*sample - 0.75).abs() < 0.001);
        }
    }

    #[test]
    fn capacity_rounds_to_power_of_two() {
        let buffer = RingBuffer::new(100);
        // 100 rounds up to 128
        assert!(buffer.free() >= 100);
    }

    #[test]
    fn multiple_write_read_cycles() {
        let buffer = RingBuffer::new(128);

        for i in 0..10 {
            let val = (i as f32) * 0.1;
            let samples = [val; 32];
            let written = buffer.write(&samples);
            assert_eq!(written, 32);

            let mut output = [0.0_f32; 32];
            let read = buffer.read(&mut output);
            assert_eq!(read, 32);
            for sample in &output {
                assert!((*sample - val).abs() < 0.001);
            }
        }
    }

    #[test]
    fn free_space_decreases_after_write() {
        let buffer = RingBuffer::new(128);
        let initial_free = buffer.free();

        let samples = [0.5_f32; 32];
        buffer.write(&samples);

        assert_eq!(buffer.free(), initial_free - 32);
    }
}
