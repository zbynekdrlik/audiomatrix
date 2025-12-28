//! Lock-free routing snapshot for real-time audio processing.
//!
//! This module provides immutable routing configuration snapshots that can be
//! read from audio callbacks without any locking. The RCU (Read-Copy-Update)
//! pattern is used: the control thread creates new snapshots on configuration
//! changes, while audio threads always read the current snapshot atomically.

use crate::atomic::AtomicF32;
use crate::destination::HeadroomMode;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Maximum number of sources that can be mixed into a single destination.
/// This is a fixed limit to avoid allocations in the audio path.
pub const MAX_SOURCES_PER_DEST: usize = 16;

/// Maximum number of destinations in a snapshot.
/// This covers typical setups with up to 64 output channels.
pub const MAX_DESTINATIONS: usize = 64;

/// A source slot in the routing matrix.
///
/// Each slot contains atomic parameters that can be modified from the control
/// thread while being read from the audio thread. The `ring_buffer_index` is
/// immutable once the slot is created (changes require a new snapshot).
#[derive(Debug)]
pub struct SourceSlot {
    /// Index into the global ring buffer pool.
    /// This is immutable once set - changing the source requires a new snapshot.
    pub ring_buffer_index: usize,

    /// Atomic gain multiplier (0.0 to ~4.0 typical).
    /// Modified by control thread, read by audio thread.
    pub gain: AtomicF32,

    /// Atomic mute flag.
    /// When true, this source contributes zero to the mix.
    pub muted: AtomicBool,

    /// Atomic enabled flag.
    /// When false, this source is completely bypassed.
    pub enabled: AtomicBool,
}

impl SourceSlot {
    /// Creates a new source slot with default parameters.
    #[must_use]
    pub fn new(ring_buffer_index: usize) -> Self {
        Self {
            ring_buffer_index,
            gain: AtomicF32::new(1.0),
            muted: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        }
    }

    /// Returns the effective gain for mixing.
    /// Returns 0.0 if muted or disabled, otherwise returns the gain value.
    #[inline]
    #[must_use]
    pub fn effective_gain(&self) -> f32 {
        if !self.enabled.load(Ordering::Relaxed) || self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            self.gain.load(Ordering::Relaxed)
        }
    }
}

impl Clone for SourceSlot {
    fn clone(&self) -> Self {
        Self {
            ring_buffer_index: self.ring_buffer_index,
            gain: AtomicF32::new(self.gain.load(Ordering::Relaxed)),
            muted: AtomicBool::new(self.muted.load(Ordering::Relaxed)),
            enabled: AtomicBool::new(self.enabled.load(Ordering::Relaxed)),
        }
    }
}

/// Atomic wrapper for HeadroomMode.
#[derive(Debug)]
pub struct AtomicHeadroomMode(AtomicU8);

impl AtomicHeadroomMode {
    /// Creates a new atomic headroom mode.
    #[must_use]
    pub fn new(mode: HeadroomMode) -> Self {
        Self(AtomicU8::new(mode as u8))
    }

    /// Loads the current mode.
    #[inline]
    #[must_use]
    pub fn load(&self) -> HeadroomMode {
        match self.0.load(Ordering::Relaxed) {
            0 => HeadroomMode::Clip,
            1 => HeadroomMode::AutoGain,
            2 => HeadroomMode::Limiter,
            3 => HeadroomMode::Manual,
            _ => HeadroomMode::Clip, // Fallback to safe default
        }
    }

    /// Stores a new mode.
    pub fn store(&self, mode: HeadroomMode) {
        self.0.store(mode as u8, Ordering::Relaxed);
    }
}

impl Clone for AtomicHeadroomMode {
    fn clone(&self) -> Self {
        Self::new(self.load())
    }
}

/// An immutable snapshot of a destination channel's routing configuration.
///
/// This struct is designed for lock-free access from audio callbacks.
/// All source slots are pre-allocated in a fixed-size array to avoid
/// any heap allocation during audio processing.
#[derive(Debug)]
pub struct DestinationSnapshot {
    /// Destination identifier (e.g., "LOCAL:DeviceName:1").
    pub dest_id: String,

    /// Device ID this destination belongs to.
    pub device_id: String,

    /// Channel index within the device (0-based).
    pub channel_index: usize,

    /// Fixed-size array of source slots.
    /// Empty slots are represented as `None`.
    pub sources: [Option<SourceSlot>; MAX_SOURCES_PER_DEST],

    /// Number of active (non-None) sources.
    /// Used to optimize iteration in the audio callback.
    pub active_count: usize,

    /// Headroom mode for this destination.
    pub headroom_mode: AtomicHeadroomMode,

    /// Manual headroom in dB (used when mode is Manual).
    pub manual_headroom_db: AtomicF32,
}

impl DestinationSnapshot {
    /// Creates a new empty destination snapshot.
    #[must_use]
    pub fn new(dest_id: String, device_id: String, channel_index: usize) -> Self {
        Self {
            dest_id,
            device_id,
            channel_index,
            sources: Default::default(),
            active_count: 0,
            headroom_mode: AtomicHeadroomMode::new(HeadroomMode::Clip),
            manual_headroom_db: AtomicF32::new(-6.0),
        }
    }

    /// Adds a source to this destination.
    /// Returns `true` if the source was added, `false` if the destination is full.
    pub fn add_source(&mut self, ring_buffer_index: usize) -> bool {
        for slot in &mut self.sources {
            if slot.is_none() {
                *slot = Some(SourceSlot::new(ring_buffer_index));
                self.active_count += 1;
                return true;
            }
        }
        false
    }

    /// Removes a source by ring buffer index.
    /// Returns `true` if the source was found and removed.
    pub fn remove_source(&mut self, ring_buffer_index: usize) -> bool {
        for slot in &mut self.sources {
            if let Some(ref source) = slot {
                if source.ring_buffer_index == ring_buffer_index {
                    *slot = None;
                    self.active_count = self.active_count.saturating_sub(1);
                    return true;
                }
            }
        }
        false
    }

    /// Finds a source slot by ring buffer index.
    #[must_use]
    pub fn find_source(&self, ring_buffer_index: usize) -> Option<&SourceSlot> {
        self.sources.iter().find_map(|slot| {
            slot.as_ref()
                .filter(|s| s.ring_buffer_index == ring_buffer_index)
        })
    }
}

impl Clone for DestinationSnapshot {
    fn clone(&self) -> Self {
        let mut sources: [Option<SourceSlot>; MAX_SOURCES_PER_DEST] = Default::default();
        for (i, slot) in self.sources.iter().enumerate() {
            sources[i] = slot.clone();
        }

        Self {
            dest_id: self.dest_id.clone(),
            device_id: self.device_id.clone(),
            channel_index: self.channel_index,
            sources,
            active_count: self.active_count,
            headroom_mode: self.headroom_mode.clone(),
            manual_headroom_db: AtomicF32::new(self.manual_headroom_db.load(Ordering::Relaxed)),
        }
    }
}

/// An immutable snapshot of the entire routing configuration.
///
/// This is what the audio thread reads - completely lock-free.
/// New snapshots are created by the control thread on configuration changes
/// and swapped in atomically using the RCU pattern.
#[derive(Debug)]
pub struct RoutingSnapshot {
    /// All destination channels indexed by position.
    pub destinations: Vec<DestinationSnapshot>,

    /// Generation counter for detecting stale snapshots.
    /// Incremented each time a new snapshot is created.
    pub generation: u64,
}

impl RoutingSnapshot {
    /// Creates a new empty routing snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            destinations: Vec::new(),
            generation: 0,
        }
    }

    /// Creates a snapshot with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            destinations: Vec::with_capacity(capacity),
            generation: 0,
        }
    }

    /// Finds a destination by ID.
    #[must_use]
    pub fn find_destination(&self, dest_id: &str) -> Option<&DestinationSnapshot> {
        self.destinations.iter().find(|d| d.dest_id == dest_id)
    }

    /// Finds a destination by ID (mutable).
    pub fn find_destination_mut(&mut self, dest_id: &str) -> Option<&mut DestinationSnapshot> {
        self.destinations.iter_mut().find(|d| d.dest_id == dest_id)
    }

    /// Gets a destination by index.
    #[inline]
    #[must_use]
    pub fn get_destination(&self, index: usize) -> Option<&DestinationSnapshot> {
        self.destinations.get(index)
    }

    /// Adds or updates a destination.
    pub fn upsert_destination(&mut self, dest: DestinationSnapshot) {
        if let Some(existing) = self.find_destination_mut(&dest.dest_id) {
            *existing = dest;
        } else {
            self.destinations.push(dest);
        }
    }

    /// Removes a destination by ID.
    pub fn remove_destination(&mut self, dest_id: &str) -> bool {
        if let Some(pos) = self.destinations.iter().position(|d| d.dest_id == dest_id) {
            self.destinations.remove(pos);
            true
        } else {
            false
        }
    }
}

impl Default for RoutingSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RoutingSnapshot {
    fn clone(&self) -> Self {
        Self {
            destinations: self.destinations.clone(),
            generation: self.generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_slot_effective_gain() {
        let slot = SourceSlot::new(0);
        assert!((slot.effective_gain() - 1.0).abs() < f32::EPSILON);

        slot.muted.store(true, Ordering::Relaxed);
        assert!(slot.effective_gain().abs() < f32::EPSILON);

        slot.muted.store(false, Ordering::Relaxed);
        slot.enabled.store(false, Ordering::Relaxed);
        assert!(slot.effective_gain().abs() < f32::EPSILON);

        slot.enabled.store(true, Ordering::Relaxed);
        slot.gain.set(0.5);
        assert!((slot.effective_gain() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn destination_add_remove_sources() {
        let mut dest = DestinationSnapshot::new(
            "test:1".to_string(),
            "test".to_string(),
            0,
        );

        assert_eq!(dest.active_count, 0);

        assert!(dest.add_source(0));
        assert_eq!(dest.active_count, 1);

        assert!(dest.add_source(1));
        assert_eq!(dest.active_count, 2);

        assert!(dest.find_source(0).is_some());
        assert!(dest.find_source(1).is_some());
        assert!(dest.find_source(2).is_none());

        assert!(dest.remove_source(0));
        assert_eq!(dest.active_count, 1);
        assert!(dest.find_source(0).is_none());

        assert!(!dest.remove_source(0)); // Already removed
    }

    #[test]
    fn destination_max_sources() {
        let mut dest = DestinationSnapshot::new(
            "test:1".to_string(),
            "test".to_string(),
            0,
        );

        for i in 0..MAX_SOURCES_PER_DEST {
            assert!(dest.add_source(i), "Failed to add source {i}");
        }

        assert_eq!(dest.active_count, MAX_SOURCES_PER_DEST);
        assert!(!dest.add_source(100)); // Should fail - full
    }

    #[test]
    fn routing_snapshot_upsert() {
        let mut snapshot = RoutingSnapshot::new();

        let dest1 = DestinationSnapshot::new(
            "dest:1".to_string(),
            "device".to_string(),
            0,
        );
        snapshot.upsert_destination(dest1);
        assert_eq!(snapshot.destinations.len(), 1);

        let dest2 = DestinationSnapshot::new(
            "dest:2".to_string(),
            "device".to_string(),
            1,
        );
        snapshot.upsert_destination(dest2);
        assert_eq!(snapshot.destinations.len(), 2);

        // Update existing
        let mut dest1_updated = DestinationSnapshot::new(
            "dest:1".to_string(),
            "device".to_string(),
            0,
        );
        dest1_updated.add_source(42);
        snapshot.upsert_destination(dest1_updated);
        assert_eq!(snapshot.destinations.len(), 2);

        let found = snapshot.find_destination("dest:1").unwrap();
        assert_eq!(found.active_count, 1);
    }

    #[test]
    fn atomic_headroom_mode() {
        let mode = AtomicHeadroomMode::new(HeadroomMode::Clip);
        assert!(matches!(mode.load(), HeadroomMode::Clip));

        mode.store(HeadroomMode::Limiter);
        assert!(matches!(mode.load(), HeadroomMode::Limiter));
    }
}
