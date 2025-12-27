//! Lock-free atomic primitives for audio processing.

use std::sync::atomic::{AtomicU32, Ordering};

/// A lock-free atomic floating-point value.
///
/// Uses `AtomicU32` internally with bit-level encoding for lock-free
/// read/write operations. This is essential for audio parameters that
/// need to be modified from the UI thread while being read from the
/// audio thread.
///
/// # Precision
///
/// Values are stored as `f32` with full precision (32-bit IEEE 754).
#[derive(Debug)]
pub struct AtomicF32 {
    bits: AtomicU32,
}

impl AtomicF32 {
    /// Creates a new `AtomicF32` with the given initial value.
    #[must_use]
    pub fn new(value: f32) -> Self {
        Self {
            bits: AtomicU32::new(value.to_bits()),
        }
    }

    /// Loads the value with the specified memory ordering.
    #[must_use]
    pub fn load(&self, ordering: Ordering) -> f32 {
        f32::from_bits(self.bits.load(ordering))
    }

    /// Stores a value with the specified memory ordering.
    pub fn store(&self, value: f32, ordering: Ordering) {
        self.bits.store(value.to_bits(), ordering);
    }

    /// Atomically swaps the value and returns the previous value.
    #[must_use]
    pub fn swap(&self, value: f32, ordering: Ordering) -> f32 {
        f32::from_bits(self.bits.swap(value.to_bits(), ordering))
    }

    /// Loads the value with `Acquire` ordering (convenience method).
    #[must_use]
    pub fn get(&self) -> f32 {
        self.load(Ordering::Acquire)
    }

    /// Stores a value with `Release` ordering (convenience method).
    pub fn set(&self, value: f32) {
        self.store(value, Ordering::Release);
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl Clone for AtomicF32 {
    fn clone(&self) -> Self {
        Self::new(self.get())
    }
}

// SAFETY: AtomicF32 uses AtomicU32 internally which is Send + Sync
unsafe impl Send for AtomicF32 {}
unsafe impl Sync for AtomicF32 {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn new_stores_initial_value() {
        let atomic = AtomicF32::new(0.5);
        assert!((atomic.get() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn default_is_zero() {
        let atomic = AtomicF32::default();
        assert!(atomic.get().abs() < f32::EPSILON);
    }

    #[test]
    fn store_load_roundtrip() {
        let atomic = AtomicF32::new(0.0);

        let values = [0.0, 0.5, 1.0, -1.0, 0.001, 1000.0, f32::MIN, f32::MAX];
        for &v in &values {
            atomic.set(v);
            assert!((atomic.get() - v).abs() < f32::EPSILON, "Failed for {v}");
        }
    }

    #[test]
    fn swap_returns_old_value() {
        let atomic = AtomicF32::new(1.0);
        let old = atomic.swap(2.0, Ordering::SeqCst);
        assert!((old - 1.0).abs() < f32::EPSILON);
        assert!((atomic.get() - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clone_creates_independent_copy() {
        let atomic1 = AtomicF32::new(1.0);
        let atomic2 = atomic1.clone();

        atomic1.set(2.0);
        assert!((atomic1.get() - 2.0).abs() < f32::EPSILON);
        assert!((atomic2.get() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn special_float_values() {
        let atomic = AtomicF32::new(f32::NAN);
        assert!(atomic.get().is_nan());

        atomic.set(f32::INFINITY);
        assert!(atomic.get().is_infinite() && atomic.get().is_sign_positive());

        atomic.set(f32::NEG_INFINITY);
        assert!(atomic.get().is_infinite() && atomic.get().is_sign_negative());
    }

    #[test]
    fn concurrent_access() {
        let atomic = Arc::new(AtomicF32::new(0.0));
        let atomic_clone = Arc::clone(&atomic);

        let writer = thread::spawn(move || {
            for i in 0..1000 {
                #[allow(clippy::cast_precision_loss)]
                atomic_clone.set(i as f32);
            }
        });

        let reader = thread::spawn({
            let atomic = Arc::clone(&atomic);
            move || {
                let mut reads = Vec::new();
                for _ in 0..1000 {
                    reads.push(atomic.get());
                }
                reads
            }
        });

        writer.join().unwrap();
        let reads = reader.join().unwrap();

        // All reads should be valid floats (not corrupted)
        for &v in &reads {
            assert!(!v.is_nan(), "Corrupted read detected");
            assert!(
                (0.0..1000.0).contains(&v),
                "Value out of expected range: {v}"
            );
        }
    }

    #[test]
    fn ordering_variants() {
        let atomic = AtomicF32::new(1.0);

        atomic.store(2.0, Ordering::Relaxed);
        assert!((atomic.load(Ordering::Relaxed) - 2.0).abs() < f32::EPSILON);

        atomic.store(3.0, Ordering::Release);
        assert!((atomic.load(Ordering::Acquire) - 3.0).abs() < f32::EPSILON);

        atomic.store(4.0, Ordering::SeqCst);
        assert!((atomic.load(Ordering::SeqCst) - 4.0).abs() < f32::EPSILON);
    }
}
