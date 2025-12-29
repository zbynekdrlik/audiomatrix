//! Pre-allocated ring buffer pool for lock-free audio routing.
//!
//! This module provides a fixed-size pool of ring buffers that are pre-allocated
//! at startup. The control thread allocates buffers for new connections, and the
//! audio thread accesses buffers by index without any locking.
//!
//! # Design
//!
//! - **Pre-allocation**: All buffers are allocated upfront to avoid heap allocations
//!   during audio processing.
//! - **Index-based access**: Buffers are accessed by index, not pointer, allowing
//!   the audio thread to safely reference buffers without synchronization.
//! - **Control thread allocation**: Only the control thread allocates/frees buffers,
//!   using a simple free list protected by a mutex.
//! - **Deferred freeing**: Buffers are not freed immediately - they're queued and
//!   only actually freed after a grace period (2 routing generations) to ensure
//!   audio callbacks have refreshed their cached routing snapshots.
//!
//! # Example
//!
//! ```
//! use ram_core::ring_buffer_pool::RingBufferPool;
//!
//! // Create pool with 64 buffers, each 1024 samples
//! let pool = RingBufferPool::new(64, 1024);
//!
//! // Control thread: allocate a buffer
//! let index = pool.allocate().expect("pool not exhausted");
//!
//! // Audio thread: access buffer by index (lock-free)
//! let buffer = pool.get(index).expect("valid index");
//! let samples = [0.5f32; 64];
//! buffer.write(&samples);
//!
//! // Control thread: defer free with current generation
//! pool.defer_free(index, 1);
//!
//! // Later, process pending frees when generation advances
//! pool.process_pending_frees(3); // Frees buffers from gen <= 1
//! ```

use crate::buffer::RingBuffer;
use parking_lot::Mutex;
use std::collections::{HashSet, VecDeque};

/// Default number of buffers in the pool.
/// Supports up to 256 concurrent connections.
pub const DEFAULT_POOL_SIZE: usize = 256;

/// Default capacity of each ring buffer in samples.
/// 2048 samples at 48kHz = ~42ms of audio per channel.
pub const DEFAULT_BUFFER_CAPACITY: usize = 2048;

/// Number of routing generations to wait before actually freeing a buffer.
/// This grace period ensures all audio callbacks have refreshed their snapshots.
const DEFER_FREE_GENERATIONS: u64 = 2;

/// A pre-allocated pool of ring buffers for audio routing.
///
/// Buffers are accessed by index for lock-free audio thread access.
/// Allocation and deallocation happen on the control thread only.
///
/// # Deferred Freeing
///
/// To prevent race conditions with audio callbacks holding cached routing
/// snapshots, buffers are not freed immediately. Instead, they're queued
/// and only actually freed after a grace period of 2 routing generations.
/// This ensures all callbacks have refreshed their snapshots before the
/// buffer is made available for reallocation.
#[derive(Debug)]
pub struct RingBufferPool {
    /// Pre-allocated ring buffers.
    /// Accessed by index from the audio thread (lock-free).
    buffers: Vec<RingBuffer>,

    /// Set of free buffer indices.
    /// Only accessed from the control thread (protected by mutex).
    free_indices: Mutex<HashSet<usize>>,

    /// Queue of buffers pending deferred free.
    /// Each entry is (buffer_index, freed_at_generation).
    /// Buffers are freed when current_generation > freed_at_generation + DEFER_FREE_GENERATIONS.
    pending_frees: Mutex<VecDeque<(usize, u64)>>,

    /// Number of buffers in the pool.
    pool_size: usize,

    /// Capacity of each buffer in samples.
    buffer_capacity: usize,
}

impl RingBufferPool {
    /// Creates a new ring buffer pool with the specified size and buffer capacity.
    ///
    /// # Arguments
    ///
    /// * `pool_size` - Number of buffers to pre-allocate
    /// * `buffer_capacity` - Capacity of each buffer in samples
    ///
    /// # Panics
    ///
    /// Panics if `pool_size` is 0.
    #[must_use]
    pub fn new(pool_size: usize, buffer_capacity: usize) -> Self {
        assert!(pool_size > 0, "pool_size must be greater than 0");

        let buffers: Vec<RingBuffer> = (0..pool_size)
            .map(|_| RingBuffer::new(buffer_capacity))
            .collect();

        let free_indices: HashSet<usize> = (0..pool_size).collect();

        Self {
            buffers,
            free_indices: Mutex::new(free_indices),
            pending_frees: Mutex::new(VecDeque::new()),
            pool_size,
            buffer_capacity,
        }
    }

    /// Creates a pool with default size and capacity.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_POOL_SIZE, DEFAULT_BUFFER_CAPACITY)
    }

    /// Returns the total number of buffers in the pool.
    #[must_use]
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Returns the capacity of each buffer in samples.
    #[must_use]
    pub fn buffer_capacity(&self) -> usize {
        self.buffer_capacity
    }

    /// Returns the number of currently allocated (in-use) buffers.
    #[must_use]
    pub fn allocated_count(&self) -> usize {
        self.pool_size - self.free_indices.lock().len()
    }

    /// Returns the number of free (available) buffers.
    #[must_use]
    pub fn free_count(&self) -> usize {
        self.free_indices.lock().len()
    }

    /// Checks if the pool is exhausted (no free buffers).
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.free_indices.lock().is_empty()
    }

    /// Allocates a buffer from the pool.
    ///
    /// This should only be called from the control thread.
    /// Returns `None` if the pool is exhausted.
    ///
    /// # Returns
    ///
    /// The index of the allocated buffer, or `None` if no buffers are available.
    pub fn allocate(&self) -> Option<usize> {
        let mut free = self.free_indices.lock();
        if let Some(&index) = free.iter().next() {
            free.remove(&index);
            // Clear the buffer before returning
            self.buffers[index].clear();
            Some(index)
        } else {
            None
        }
    }

    /// Frees a buffer back to the pool immediately.
    ///
    /// **WARNING**: This bypasses deferred freeing and should only be used
    /// during shutdown or when you're certain no audio callbacks are running.
    /// For normal route removal, use `defer_free()` instead.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the buffer to free
    ///
    /// # Returns
    ///
    /// `true` if the buffer was successfully freed, `false` if the index was
    /// invalid or the buffer was already free.
    pub fn free(&self, index: usize) -> bool {
        if index >= self.pool_size {
            return false;
        }

        let mut free = self.free_indices.lock();
        if free.contains(&index) {
            // Already free - this is a double-free bug
            return false;
        }

        // Clear the buffer and return it to the pool
        self.buffers[index].clear();
        free.insert(index);
        true
    }

    /// Queues a buffer for deferred freeing.
    ///
    /// This is the safe way to free buffers during normal operation. The buffer
    /// is not immediately freed - it's queued and will only be freed after
    /// `DEFER_FREE_GENERATIONS` routing generations have passed. This ensures
    /// that all audio callbacks have had time to refresh their cached routing
    /// snapshots and no longer reference this buffer.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the buffer to free
    /// * `current_generation` - The current routing table generation
    ///
    /// # Returns
    ///
    /// `true` if the buffer was queued for deferred freeing, `false` if the
    /// index was invalid.
    pub fn defer_free(&self, index: usize, current_generation: u64) -> bool {
        if index >= self.pool_size {
            return false;
        }

        // Check if already free or pending
        {
            let free = self.free_indices.lock();
            if free.contains(&index) {
                return false; // Already free
            }
        }

        // Add to pending queue
        let mut pending = self.pending_frees.lock();
        pending.push_back((index, current_generation));
        true
    }

    /// Processes pending buffer frees, actually freeing buffers that have
    /// passed the grace period.
    ///
    /// This should be called periodically from the control thread (e.g., at
    /// the start of each route add/remove operation).
    ///
    /// # Arguments
    ///
    /// * `current_generation` - The current routing table generation
    ///
    /// # Returns
    ///
    /// The number of buffers that were actually freed.
    pub fn process_pending_frees(&self, current_generation: u64) -> usize {
        let mut pending = self.pending_frees.lock();
        let mut freed_count = 0;

        // Process from the front of the queue (oldest entries first)
        while let Some(&(index, freed_at_gen)) = pending.front() {
            // Check if enough generations have passed
            if current_generation > freed_at_gen + DEFER_FREE_GENERATIONS {
                pending.pop_front();
                // Actually free the buffer now
                self.buffers[index].clear();
                self.free_indices.lock().insert(index);
                freed_count += 1;
            } else {
                // Pending entries are in order, so if this one isn't ready, none after it are
                break;
            }
        }

        freed_count
    }

    /// Returns the number of buffers pending deferred free.
    #[must_use]
    pub fn pending_free_count(&self) -> usize {
        self.pending_frees.lock().len()
    }

    /// Gets a reference to a buffer by index.
    ///
    /// This is the primary access path for the audio thread and is lock-free.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the buffer to access
    ///
    /// # Returns
    ///
    /// A reference to the buffer, or `None` if the index is out of bounds.
    ///
    /// # Note
    ///
    /// This method does not check if the buffer is allocated. It is the caller's
    /// responsibility to only access allocated buffers. Accessing a freed buffer
    /// is safe (no UB) but may result in stale or corrupted audio data.
    #[inline]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&RingBuffer> {
        self.buffers.get(index)
    }

    /// Checks if a buffer index is currently allocated (in use).
    ///
    /// This acquires a lock and should not be called from the audio thread.
    #[must_use]
    pub fn is_allocated(&self, index: usize) -> bool {
        if index >= self.pool_size {
            return false;
        }
        !self.free_indices.lock().contains(&index)
    }

    /// Returns all currently allocated buffer indices.
    ///
    /// This is useful for debugging and cleanup operations.
    #[must_use]
    pub fn allocated_indices(&self) -> Vec<usize> {
        let free = self.free_indices.lock();
        (0..self.pool_size).filter(|i| !free.contains(i)).collect()
    }

    /// Frees all allocated buffers immediately.
    ///
    /// This should only be called during shutdown or reset when no audio
    /// callbacks are running. It also clears any pending deferred frees.
    pub fn free_all(&self) {
        // Clear pending frees
        self.pending_frees.lock().clear();

        // Free all buffers
        let mut free = self.free_indices.lock();
        for i in 0..self.pool_size {
            self.buffers[i].clear();
            free.insert(i);
        }
    }
}

impl Default for RingBufferPool {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// Safety: RingBufferPool can be safely shared between threads
// - Vec<RingBuffer> is Send + Sync (RingBuffer is Send + Sync)
// - Mutex<HashSet<usize>> is Send + Sync
unsafe impl Send for RingBufferPool {}
unsafe impl Sync for RingBufferPool {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn new_pool_all_free() {
        let pool = RingBufferPool::new(8, 256);
        assert_eq!(pool.pool_size(), 8);
        assert_eq!(pool.buffer_capacity(), 256);
        assert_eq!(pool.free_count(), 8);
        assert_eq!(pool.allocated_count(), 0);
        assert!(!pool.is_exhausted());
    }

    #[test]
    fn allocate_returns_index() {
        let pool = RingBufferPool::new(8, 256);

        let index = pool.allocate();
        assert!(index.is_some());
        assert!(index.unwrap() < 8);

        assert_eq!(pool.allocated_count(), 1);
        assert_eq!(pool.free_count(), 7);
    }

    #[test]
    fn allocate_all_exhausts_pool() {
        let pool = RingBufferPool::new(4, 256);

        for _ in 0..4 {
            assert!(pool.allocate().is_some());
        }

        assert!(pool.is_exhausted());
        assert!(pool.allocate().is_none());
    }

    #[test]
    fn free_returns_buffer_to_pool() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        assert_eq!(pool.free_count(), 3);

        assert!(pool.free(index));
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn free_invalid_index_fails() {
        let pool = RingBufferPool::new(4, 256);
        assert!(!pool.free(999));
    }

    #[test]
    fn double_free_fails() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        assert!(pool.free(index));
        assert!(!pool.free(index)); // Double free should fail
    }

    #[test]
    fn get_returns_buffer() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        let buffer = pool.get(index);
        assert!(buffer.is_some());

        // Write to the buffer
        let buffer = buffer.unwrap();
        let samples = [0.5f32; 64];
        assert_eq!(buffer.write(&samples), 64);
    }

    #[test]
    fn get_invalid_index_returns_none() {
        let pool = RingBufferPool::new(4, 256);
        assert!(pool.get(999).is_none());
    }

    #[test]
    fn is_allocated_works() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        assert!(pool.is_allocated(index));

        pool.free(index);
        assert!(!pool.is_allocated(index));
    }

    #[test]
    fn allocated_indices_returns_correct_set() {
        let pool = RingBufferPool::new(8, 256);

        let idx1 = pool.allocate().unwrap();
        let idx2 = pool.allocate().unwrap();
        let _idx3 = pool.allocate().unwrap();

        pool.free(idx2);

        let allocated = pool.allocated_indices();
        assert_eq!(allocated.len(), 2);
        assert!(allocated.contains(&idx1));
        assert!(!allocated.contains(&idx2));
    }

    #[test]
    fn free_all_clears_pool() {
        let pool = RingBufferPool::new(4, 256);

        pool.allocate();
        pool.allocate();
        pool.allocate();

        assert_eq!(pool.allocated_count(), 3);

        pool.free_all();

        assert_eq!(pool.allocated_count(), 0);
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn allocate_clears_buffer() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        let buffer = pool.get(index).unwrap();

        // Write data
        let samples = [0.5f32; 64];
        buffer.write(&samples);
        assert_eq!(buffer.available(), 64);

        // Free and reallocate
        pool.free(index);
        let new_index = pool.allocate().unwrap();

        // Buffer should be cleared
        let new_buffer = pool.get(new_index).unwrap();
        assert_eq!(new_buffer.available(), 0);
    }

    #[test]
    fn concurrent_allocate_free() {
        let pool = Arc::new(RingBufferPool::new(64, 256));
        let iterations = 100;

        // Spawn threads that allocate and free
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let pool = Arc::clone(&pool);
                thread::spawn(move || {
                    for _ in 0..iterations {
                        if let Some(index) = pool.allocate() {
                            // Use the buffer briefly
                            if let Some(buffer) = pool.get(index) {
                                let samples = [0.1f32; 16];
                                buffer.write(&samples);
                            }
                            pool.free(index);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All buffers should be free at the end
        assert_eq!(pool.free_count(), 64);
    }

    #[test]
    fn audio_thread_access_while_control_allocates() {
        let pool = Arc::new(RingBufferPool::new(16, 256));

        // Pre-allocate some buffers
        let indices: Vec<usize> = (0..8).filter_map(|_| pool.allocate()).collect();

        // "Audio thread" reads from allocated buffers
        let pool_audio = Arc::clone(&pool);
        let indices_audio = indices.clone();
        let audio_handle = thread::spawn(move || {
            for _ in 0..1000 {
                for &index in &indices_audio {
                    // Lock-free access
                    if let Some(buffer) = pool_audio.get(index) {
                        // Read available (this is atomic)
                        let _ = buffer.available();
                    }
                }
            }
        });

        // "Control thread" allocates and frees other buffers
        let pool_control = Arc::clone(&pool);
        let control_handle = thread::spawn(move || {
            for _ in 0..100 {
                if let Some(index) = pool_control.allocate() {
                    thread::yield_now();
                    pool_control.free(index);
                }
            }
        });

        audio_handle.join().unwrap();
        control_handle.join().unwrap();

        // Clean up pre-allocated buffers
        for index in indices {
            pool.free(index);
        }

        assert_eq!(pool.free_count(), 16);
    }

    #[test]
    fn with_defaults_creates_expected_pool() {
        let pool = RingBufferPool::with_defaults();
        assert_eq!(pool.pool_size(), DEFAULT_POOL_SIZE);
        assert_eq!(pool.buffer_capacity(), DEFAULT_BUFFER_CAPACITY);
    }

    #[test]
    #[should_panic(expected = "pool_size must be greater than 0")]
    fn zero_pool_size_panics() {
        let _ = RingBufferPool::new(0, 256);
    }

    #[test]
    fn defer_free_queues_buffer() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        assert!(pool.is_allocated(index));
        assert_eq!(pool.pending_free_count(), 0);

        // Defer free at generation 1
        assert!(pool.defer_free(index, 1));
        // Buffer is still allocated (not in free list yet)
        assert!(pool.is_allocated(index));
        // But it's pending
        assert_eq!(pool.pending_free_count(), 1);
    }

    #[test]
    fn process_pending_frees_respects_grace_period() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        pool.defer_free(index, 1); // Freed at generation 1

        // At generation 2 (grace period = 2, need > 1+2 = 3 to free)
        assert_eq!(pool.process_pending_frees(2), 0);
        assert!(pool.is_allocated(index));

        // At generation 3 (still not > 3)
        assert_eq!(pool.process_pending_frees(3), 0);
        assert!(pool.is_allocated(index));

        // At generation 4 (now > 3, should free)
        assert_eq!(pool.process_pending_frees(4), 1);
        assert!(!pool.is_allocated(index));
    }

    #[test]
    fn process_pending_frees_multiple_buffers() {
        let pool = RingBufferPool::new(8, 256);

        // Allocate and defer free multiple buffers at different generations
        let idx1 = pool.allocate().unwrap();
        let idx2 = pool.allocate().unwrap();
        let idx3 = pool.allocate().unwrap();

        pool.defer_free(idx1, 1);
        pool.defer_free(idx2, 2);
        pool.defer_free(idx3, 5);

        assert_eq!(pool.pending_free_count(), 3);

        // At generation 4: idx1 should be freed (4 > 1+2)
        assert_eq!(pool.process_pending_frees(4), 1);
        assert!(!pool.is_allocated(idx1));
        assert!(pool.is_allocated(idx2)); // 4 not > 2+2=4
        assert!(pool.is_allocated(idx3)); // 4 not > 5+2=7

        // At generation 5: idx2 should be freed (5 > 2+2=4)
        assert_eq!(pool.process_pending_frees(5), 1);
        assert!(!pool.is_allocated(idx2));
        assert!(pool.is_allocated(idx3)); // 5 not > 5+2=7

        // At generation 8: idx3 should be freed (8 > 5+2=7)
        assert_eq!(pool.process_pending_frees(8), 1);
        assert!(!pool.is_allocated(idx3));

        assert_eq!(pool.pending_free_count(), 0);
    }

    #[test]
    fn defer_free_invalid_index_returns_false() {
        let pool = RingBufferPool::new(4, 256);
        assert!(!pool.defer_free(999, 1));
    }

    #[test]
    fn defer_free_already_free_returns_false() {
        let pool = RingBufferPool::new(4, 256);

        let index = pool.allocate().unwrap();
        pool.free(index); // Immediately free

        // Trying to defer free an already-freed buffer should fail
        assert!(!pool.defer_free(index, 1));
    }

    #[test]
    fn free_all_clears_pending() {
        let pool = RingBufferPool::new(4, 256);

        let idx1 = pool.allocate().unwrap();
        let idx2 = pool.allocate().unwrap();

        pool.defer_free(idx1, 1);
        pool.defer_free(idx2, 1);

        assert_eq!(pool.pending_free_count(), 2);
        assert_eq!(pool.free_count(), 2); // Only 2 free

        pool.free_all();

        assert_eq!(pool.pending_free_count(), 0);
        assert_eq!(pool.free_count(), 4); // All 4 free
    }
}
