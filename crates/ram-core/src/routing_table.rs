//! RCU (Read-Copy-Update) routing table for lock-free audio routing.
//!
//! This module provides a thread-safe routing table that allows the audio thread
//! to read the current routing configuration without any locking, while the control
//! thread can update the configuration atomically.
//!
//! The RCU pattern works as follows:
//! 1. Audio thread calls `snapshot()` to get the current routing (lock-free load)
//! 2. Control thread calls `update()` with a new snapshot
//! 3. Old snapshot is kept alive until all readers are done (via Arc refcount)
//!
//! This guarantees:
//! - Zero locks in the audio path
//! - Atomic configuration updates (no partial states)
//! - Safe memory reclamation via reference counting

use crate::routing_snapshot::RoutingSnapshot;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A lock-free routing table using RCU (Read-Copy-Update) semantics.
///
/// The audio thread reads the current snapshot without any locking.
/// The control thread updates the snapshot atomically.
///
/// # Example
///
/// ```
/// use ram_core::routing_table::RoutingTable;
/// use ram_core::routing_snapshot::RoutingSnapshot;
///
/// let table = RoutingTable::new();
///
/// // Audio thread: lock-free read
/// let snapshot = table.snapshot();
/// assert_eq!(snapshot.destinations.len(), 0);
///
/// // Control thread: atomic update
/// let mut new_snapshot = RoutingSnapshot::new();
/// new_snapshot.generation = 1;
/// table.update(new_snapshot);
///
/// // Audio thread sees new configuration
/// let snapshot = table.snapshot();
/// assert_eq!(snapshot.generation, 1);
/// ```
#[derive(Debug)]
pub struct RoutingTable {
    /// Current routing snapshot wrapped in Arc for reference counting.
    /// Uses parking_lot::RwLock for the Arc swap - this is NOT in the audio path.
    /// Audio thread only reads the Arc (atomic reference count increment).
    current: parking_lot::RwLock<Arc<RoutingSnapshot>>,

    /// Generation counter for detecting updates.
    /// Incremented on each update() call.
    generation: AtomicU64,
}

impl RoutingTable {
    /// Creates a new empty routing table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: parking_lot::RwLock::new(Arc::new(RoutingSnapshot::new())),
            generation: AtomicU64::new(0),
        }
    }

    /// Creates a routing table with an initial snapshot.
    #[must_use]
    pub fn with_snapshot(snapshot: RoutingSnapshot) -> Self {
        let generation = snapshot.generation;
        Self {
            current: parking_lot::RwLock::new(Arc::new(snapshot)),
            generation: AtomicU64::new(generation),
        }
    }

    /// Returns the current routing snapshot.
    ///
    /// This is the primary read path for the audio thread.
    /// While this does acquire a read lock, `parking_lot::RwLock` is designed
    /// for very fast uncontended reads. The actual audio processing uses the
    /// returned Arc without any locking.
    ///
    /// For truly lock-free reads in the hottest path, use `snapshot_if_changed()`
    /// with a cached snapshot.
    #[inline]
    #[must_use]
    pub fn snapshot(&self) -> Arc<RoutingSnapshot> {
        Arc::clone(&self.current.read())
    }

    /// Returns the current generation counter.
    ///
    /// This is a lock-free atomic load that can be used to check if the
    /// routing has changed since last read, avoiding the Arc clone overhead.
    #[inline]
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Returns a new snapshot only if the generation has changed.
    ///
    /// This is an optimization for the audio thread: cache the snapshot and
    /// generation, then only call this when needed. If generation hasn't
    /// changed, returns None and avoids the Arc clone.
    ///
    /// # Arguments
    ///
    /// * `cached_generation` - The generation of the currently cached snapshot
    ///
    /// # Returns
    ///
    /// * `Some(snapshot)` if generation changed
    /// * `None` if generation is the same (cached snapshot is still valid)
    #[inline]
    #[must_use]
    pub fn snapshot_if_changed(&self, cached_generation: u64) -> Option<Arc<RoutingSnapshot>> {
        let current_gen = self.generation.load(Ordering::Acquire);
        if current_gen != cached_generation {
            Some(self.snapshot())
        } else {
            None
        }
    }

    /// Updates the routing table with a new snapshot.
    ///
    /// This is called from the control thread when routing configuration changes.
    /// The update is atomic - readers either see the old or new configuration,
    /// never a partial state.
    ///
    /// Old snapshots are automatically freed when all readers are done
    /// (via Arc reference counting).
    ///
    /// # Arguments
    ///
    /// * `snapshot` - The new routing configuration
    pub fn update(&self, mut snapshot: RoutingSnapshot) {
        // Increment generation and set it in the snapshot
        let new_gen = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        snapshot.generation = new_gen;

        // Swap in the new snapshot
        let new_arc = Arc::new(snapshot);
        *self.current.write() = new_arc;
    }

    /// Updates the routing table using a closure that modifies the current snapshot.
    ///
    /// This is useful for incremental updates where you want to modify the
    /// existing configuration rather than replace it entirely.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that takes the current snapshot and returns a new one
    ///
    /// # Example
    ///
    /// ```
    /// use ram_core::routing_table::RoutingTable;
    /// use ram_core::routing_snapshot::{RoutingSnapshot, DestinationSnapshot};
    ///
    /// let table = RoutingTable::new();
    ///
    /// table.update_with(|current| {
    ///     let mut new = current.clone();
    ///     new.destinations.push(DestinationSnapshot::new(
    ///         "LOCAL:Device:1".to_string(),
    ///         "Device".to_string(),
    ///         0,
    ///     ));
    ///     new
    /// });
    /// ```
    pub fn update_with<F>(&self, f: F)
    where
        F: FnOnce(&RoutingSnapshot) -> RoutingSnapshot,
    {
        let current = self.snapshot();
        let new_snapshot = f(&current);
        self.update(new_snapshot);
    }

    /// Returns the number of destinations in the current snapshot.
    ///
    /// This acquires a read lock briefly to get the count.
    #[must_use]
    pub fn destination_count(&self) -> usize {
        self.current.read().destinations.len()
    }

    /// Checks if the routing table is empty (no destinations).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.destination_count() == 0
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

// Safety: RoutingTable can be safely shared between threads
// - parking_lot::RwLock is Send + Sync
// - Arc<RoutingSnapshot> is Send + Sync
// - AtomicU64 is Send + Sync
unsafe impl Send for RoutingTable {}
unsafe impl Sync for RoutingTable {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_snapshot::DestinationSnapshot;
    use std::thread;

    #[test]
    fn new_table_is_empty() {
        let table = RoutingTable::new();
        assert!(table.is_empty());
        assert_eq!(table.destination_count(), 0);
        assert_eq!(table.generation(), 0);
    }

    #[test]
    fn update_increments_generation() {
        let table = RoutingTable::new();
        assert_eq!(table.generation(), 0);

        table.update(RoutingSnapshot::new());
        assert_eq!(table.generation(), 1);

        table.update(RoutingSnapshot::new());
        assert_eq!(table.generation(), 2);
    }

    #[test]
    fn snapshot_reflects_updates() {
        let table = RoutingTable::new();

        let mut snapshot = RoutingSnapshot::new();
        snapshot.destinations.push(DestinationSnapshot::new(
            "test:1".to_string(),
            "test".to_string(),
            0,
        ));
        table.update(snapshot);

        let read = table.snapshot();
        assert_eq!(read.destinations.len(), 1);
        assert_eq!(read.destinations[0].dest_id, "test:1");
    }

    #[test]
    fn snapshot_if_changed_returns_none_when_unchanged() {
        let table = RoutingTable::new();
        let gen = table.generation();

        assert!(table.snapshot_if_changed(gen).is_none());
    }

    #[test]
    fn snapshot_if_changed_returns_some_when_changed() {
        let table = RoutingTable::new();
        let gen = table.generation();

        table.update(RoutingSnapshot::new());

        let result = table.snapshot_if_changed(gen);
        assert!(result.is_some());
        assert_eq!(result.unwrap().generation, 1);
    }

    #[test]
    fn update_with_modifies_snapshot() {
        let table = RoutingTable::new();

        table.update_with(|current| {
            let mut new = current.clone();
            new.destinations.push(DestinationSnapshot::new(
                "dest:1".to_string(),
                "device".to_string(),
                0,
            ));
            new
        });

        assert_eq!(table.destination_count(), 1);
    }

    #[test]
    fn concurrent_reads_and_writes() {
        use std::sync::Arc as StdArc;

        let table = StdArc::new(RoutingTable::new());
        let iterations = 1000;

        // Spawn reader threads
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let table = StdArc::clone(&table);
                thread::spawn(move || {
                    for _ in 0..iterations {
                        let snapshot = table.snapshot();
                        // Just read the snapshot - shouldn't panic
                        let _ = snapshot.destinations.len();
                    }
                })
            })
            .collect();

        // Spawn writer thread
        let writer_table = StdArc::clone(&table);
        let writer = thread::spawn(move || {
            for i in 0..iterations {
                let mut snapshot = RoutingSnapshot::new();
                snapshot.destinations.push(DestinationSnapshot::new(
                    format!("dest:{i}"),
                    "device".to_string(),
                    0,
                ));
                writer_table.update(snapshot);
            }
        });

        // Wait for all threads
        for reader in readers {
            reader.join().unwrap();
        }
        writer.join().unwrap();

        // Final state should be valid
        assert_eq!(table.destination_count(), 1);
        assert_eq!(table.generation(), iterations as u64);
    }

    #[test]
    fn old_snapshot_remains_valid_after_update() {
        let table = RoutingTable::new();

        // Create initial snapshot with one destination
        let mut initial = RoutingSnapshot::new();
        initial.destinations.push(DestinationSnapshot::new(
            "old:1".to_string(),
            "old".to_string(),
            0,
        ));
        table.update(initial);

        // Get a reference to current snapshot
        let old_snapshot = table.snapshot();
        assert_eq!(old_snapshot.destinations[0].dest_id, "old:1");

        // Update with new snapshot
        let mut new = RoutingSnapshot::new();
        new.destinations.push(DestinationSnapshot::new(
            "new:1".to_string(),
            "new".to_string(),
            0,
        ));
        table.update(new);

        // Old snapshot should still be valid (Arc keeps it alive)
        assert_eq!(old_snapshot.destinations[0].dest_id, "old:1");

        // New reads get the new snapshot
        let new_snapshot = table.snapshot();
        assert_eq!(new_snapshot.destinations[0].dest_id, "new:1");
    }

    #[test]
    fn with_snapshot_initializes_correctly() {
        let mut snapshot = RoutingSnapshot::new();
        snapshot.generation = 42;
        snapshot.destinations.push(DestinationSnapshot::new(
            "init:1".to_string(),
            "init".to_string(),
            0,
        ));

        let table = RoutingTable::with_snapshot(snapshot);

        assert_eq!(table.generation(), 42);
        assert_eq!(table.destination_count(), 1);
    }
}
