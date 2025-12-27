//! Lock-free packet pool for allocation-free VBAN transmission.
//!
//! This module provides pre-allocated packet buffers that can be acquired
//! and released without heap allocation in the audio path.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::MAX_PACKET_SIZE;

/// A single packet buffer in the pool.
#[derive(Debug)]
pub struct PacketBuffer {
    /// The pre-allocated buffer.
    data: [u8; MAX_PACKET_SIZE],
    /// Current length of valid data.
    len: usize,
}

impl PacketBuffer {
    /// Creates a new empty packet buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            data: [0u8; MAX_PACKET_SIZE],
            len: 0,
        }
    }

    /// Returns the buffer data as a mutable slice.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Returns the valid data portion of the buffer.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }

    /// Sets the length of valid data in the buffer.
    ///
    /// If `len` exceeds `MAX_PACKET_SIZE`, it will be clamped.
    pub fn set_len(&mut self, len: usize) {
        self.len = len.min(MAX_PACKET_SIZE);
    }

    /// Returns the current length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the buffer is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl Default for PacketBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// A slot in the packet pool that tracks whether it's in use.
struct PoolSlot {
    /// The packet buffer.
    buffer: PacketBuffer,
    /// Whether this slot is currently in use.
    in_use: AtomicBool,
}

impl PoolSlot {
    fn new() -> Self {
        Self {
            buffer: PacketBuffer::new(),
            in_use: AtomicBool::new(false),
        }
    }

    /// Tries to acquire this slot. Returns true if successful.
    fn try_acquire(&self) -> bool {
        self.in_use
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    /// Releases this slot back to the pool.
    fn release(&self) {
        self.in_use.store(false, Ordering::Release);
    }
}

/// A handle to a borrowed packet buffer.
///
/// When dropped, the buffer is automatically returned to the pool.
pub struct PooledPacket<'a> {
    slot_index: usize,
    pool: &'a PacketPool,
}

impl PooledPacket<'_> {
    /// Returns the underlying buffer for reading/writing.
    #[must_use]
    pub fn buffer(&mut self) -> &mut PacketBuffer {
        // SAFETY: We have exclusive access through the atomic flag
        unsafe { &mut (*self.pool.slots.as_ptr().cast_mut().add(self.slot_index)).buffer }
    }

    /// Returns the buffer data.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        // SAFETY: We have exclusive access through the atomic flag
        unsafe {
            (*self.pool.slots.as_ptr().add(self.slot_index))
                .buffer
                .data()
        }
    }
}

impl Drop for PooledPacket<'_> {
    fn drop(&mut self) {
        // SAFETY: We have exclusive access through the atomic flag
        unsafe {
            let slot = self.pool.slots.as_ptr().cast_mut().add(self.slot_index);
            (*slot).buffer.clear();
            (*slot).release();
        }
        self.pool.available.fetch_add(1, Ordering::Release);
    }
}

/// A lock-free pool of pre-allocated packet buffers.
///
/// This pool allows acquiring and releasing packet buffers without
/// heap allocation, which is essential for real-time audio processing.
pub struct PacketPool {
    /// The pre-allocated slots.
    slots: Box<[PoolSlot]>,
    /// Number of available slots.
    available: AtomicUsize,
    /// Hint for where to start searching.
    search_hint: AtomicUsize,
}

impl PacketPool {
    /// Creates a new packet pool with the specified capacity.
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "pool capacity must be > 0");

        let slots: Vec<PoolSlot> = (0..capacity).map(|_| PoolSlot::new()).collect();

        Self {
            slots: slots.into_boxed_slice(),
            available: AtomicUsize::new(capacity),
            search_hint: AtomicUsize::new(0),
        }
    }

    /// Returns the total capacity of the pool.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Returns the number of available buffers.
    #[must_use]
    pub fn available(&self) -> usize {
        self.available.load(Ordering::Acquire)
    }

    /// Tries to acquire a packet buffer from the pool.
    ///
    /// Returns `None` if no buffers are available (pool exhausted).
    /// This operation is lock-free and allocation-free.
    #[must_use]
    pub fn try_acquire(&self) -> Option<PooledPacket<'_>> {
        // Quick check if any buffers are available
        if self.available.load(Ordering::Acquire) == 0 {
            return None;
        }

        let capacity = self.slots.len();
        let start = self.search_hint.load(Ordering::Relaxed) % capacity;

        // Search from hint position, wrapping around
        for i in 0..capacity {
            let index = (start + i) % capacity;
            if self.slots[index].try_acquire() {
                self.available.fetch_sub(1, Ordering::Release);
                // Update hint to next position for better distribution
                self.search_hint
                    .store((index + 1) % capacity, Ordering::Relaxed);
                return Some(PooledPacket {
                    slot_index: index,
                    pool: self,
                });
            }
        }

        None
    }
}

// SAFETY: The pool uses atomic operations for thread safety
unsafe impl Send for PacketPool {}
unsafe impl Sync for PacketPool {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn packet_buffer_basic() {
        let mut buf = PacketBuffer::new();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());

        buf.as_mut_slice()[0..4].copy_from_slice(&[1, 2, 3, 4]);
        buf.set_len(4);

        assert_eq!(buf.len(), 4);
        assert!(!buf.is_empty());
        assert_eq!(buf.data(), &[1, 2, 3, 4]);

        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn packet_buffer_max_len() {
        let mut buf = PacketBuffer::new();
        buf.set_len(MAX_PACKET_SIZE + 100);
        assert_eq!(buf.len(), MAX_PACKET_SIZE);
    }

    #[test]
    fn pool_new() {
        let pool = PacketPool::new(8);
        assert_eq!(pool.capacity(), 8);
        assert_eq!(pool.available(), 8);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn pool_zero_capacity_panics() {
        let _ = PacketPool::new(0);
    }

    #[test]
    fn pool_acquire_release() {
        let pool = PacketPool::new(4);

        {
            let mut packet = pool.try_acquire().expect("should acquire");
            assert_eq!(pool.available(), 3);

            packet.buffer().as_mut_slice()[0..4].copy_from_slice(b"test");
            packet.buffer().set_len(4);
            assert_eq!(packet.data(), b"test");
        }

        // Should be released back
        assert_eq!(pool.available(), 4);
    }

    #[test]
    fn pool_exhaustion() {
        let pool = PacketPool::new(2);

        let p1 = pool.try_acquire();
        let p2 = pool.try_acquire();
        let p3 = pool.try_acquire();

        assert!(p1.is_some());
        assert!(p2.is_some());
        assert!(p3.is_none()); // Pool exhausted

        drop(p1);
        let p4 = pool.try_acquire();
        assert!(p4.is_some()); // Now available again
    }

    #[test]
    fn pool_concurrent_access() {
        let pool = Arc::new(PacketPool::new(32));
        let mut handles = vec![];

        for _ in 0..8 {
            let pool_clone = Arc::clone(&pool);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    if let Some(mut packet) = pool_clone.try_acquire() {
                        packet.buffer().set_len(64);
                        // Simulate some work
                        std::hint::black_box(packet.data());
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All packets should be returned
        assert_eq!(pool.available(), 32);
    }

    #[test]
    fn pool_search_hint_distribution() {
        let pool = PacketPool::new(4);

        // Acquire and release multiple times to test hint distribution
        for _ in 0..16 {
            let packet = pool.try_acquire().unwrap();
            drop(packet);
        }

        assert_eq!(pool.available(), 4);
    }
}
