//! Stream registry for managing active audio streams.
//!
//! The stream registry keeps track of all active input and output streams,
//! organized by device ID. It provides methods to register, lookup, and
//! unregister streams as audio processing starts and stops.
//!
//! # Thread Safety
//!
//! The registry uses parking_lot::RwLock for thread-safe access.
//! Operations that modify the registry (register/unregister) acquire a write lock,
//! while read operations (lookup/iterate) acquire a read lock.

use crate::active_stream::{ActiveInputStream, ActiveOutputStream};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for managing all active audio streams.
///
/// Streams are organized by device ID for efficient lookup when routing
/// needs to find which streams are active for a given device.
#[derive(Debug, Default)]
pub struct StreamRegistry {
    /// Active input streams, keyed by device ID.
    input_streams: RwLock<HashMap<String, Arc<ActiveInputStream>>>,
    /// Active output streams, keyed by device ID.
    output_streams: RwLock<HashMap<String, Arc<ActiveOutputStream>>>,
}

impl StreamRegistry {
    /// Creates a new empty stream registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // === Input Stream Operations ===

    /// Registers an input stream.
    ///
    /// If a stream for this device already exists, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `stream` - The active input stream to register
    pub fn register_input(&self, stream: Arc<ActiveInputStream>) {
        let device_id = stream.device_id().to_string();
        self.input_streams.write().insert(device_id, stream);
    }

    /// Unregisters an input stream by device ID.
    ///
    /// # Returns
    ///
    /// The removed stream, if it existed.
    pub fn unregister_input(&self, device_id: &str) -> Option<Arc<ActiveInputStream>> {
        self.input_streams.write().remove(device_id)
    }

    /// Gets an input stream by device ID.
    #[must_use]
    pub fn get_input(&self, device_id: &str) -> Option<Arc<ActiveInputStream>> {
        self.input_streams.read().get(device_id).cloned()
    }

    /// Checks if an input stream exists for the given device.
    #[must_use]
    pub fn has_input(&self, device_id: &str) -> bool {
        self.input_streams.read().contains_key(device_id)
    }

    /// Returns the number of active input streams.
    #[must_use]
    pub fn input_count(&self) -> usize {
        self.input_streams.read().len()
    }

    /// Returns all active input device IDs.
    #[must_use]
    pub fn input_device_ids(&self) -> Vec<String> {
        self.input_streams.read().keys().cloned().collect()
    }

    // === Output Stream Operations ===

    /// Registers an output stream.
    ///
    /// If a stream for this device already exists, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `stream` - The active output stream to register
    pub fn register_output(&self, stream: Arc<ActiveOutputStream>) {
        let device_id = stream.device_id().to_string();
        self.output_streams.write().insert(device_id, stream);
    }

    /// Unregisters an output stream by device ID.
    ///
    /// # Returns
    ///
    /// The removed stream, if it existed.
    pub fn unregister_output(&self, device_id: &str) -> Option<Arc<ActiveOutputStream>> {
        self.output_streams.write().remove(device_id)
    }

    /// Gets an output stream by device ID.
    #[must_use]
    pub fn get_output(&self, device_id: &str) -> Option<Arc<ActiveOutputStream>> {
        self.output_streams.read().get(device_id).cloned()
    }

    /// Checks if an output stream exists for the given device.
    #[must_use]
    pub fn has_output(&self, device_id: &str) -> bool {
        self.output_streams.read().contains_key(device_id)
    }

    /// Returns the number of active output streams.
    #[must_use]
    pub fn output_count(&self) -> usize {
        self.output_streams.read().len()
    }

    /// Returns all active output device IDs.
    #[must_use]
    pub fn output_device_ids(&self) -> Vec<String> {
        self.output_streams.read().keys().cloned().collect()
    }

    // === Combined Operations ===

    /// Returns the total number of active streams (input + output).
    #[must_use]
    pub fn total_count(&self) -> usize {
        self.input_count() + self.output_count()
    }

    /// Checks if the registry is empty (no active streams).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }

    /// Clears all streams from the registry.
    ///
    /// This should be called during shutdown to ensure all streams are properly stopped.
    pub fn clear(&self) {
        self.input_streams.write().clear();
        self.output_streams.write().clear();
    }

    /// Gets aggregate statistics for all input streams.
    #[must_use]
    pub fn input_stats(&self) -> AggregateStats {
        let streams = self.input_streams.read();
        let mut stats = AggregateStats::default();
        for stream in streams.values() {
            let s = stream.stats();
            stats.total_callbacks += s.callbacks();
            stats.total_underruns += s.underruns();
            stats.total_overruns += s.overruns();
            stats.stream_count += 1;
        }
        stats
    }

    /// Gets aggregate statistics for all output streams.
    #[must_use]
    pub fn output_stats(&self) -> AggregateStats {
        let streams = self.output_streams.read();
        let mut stats = AggregateStats::default();
        for stream in streams.values() {
            let s = stream.stats();
            stats.total_callbacks += s.callbacks();
            stats.total_underruns += s.underruns();
            stats.total_overruns += s.overruns();
            stats.stream_count += 1;
        }
        stats
    }

    /// Iterates over all input streams, calling the provided closure.
    pub fn for_each_input<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<ActiveInputStream>),
    {
        for stream in self.input_streams.read().values() {
            f(stream);
        }
    }

    /// Iterates over all output streams, calling the provided closure.
    pub fn for_each_output<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<ActiveOutputStream>),
    {
        for stream in self.output_streams.read().values() {
            f(stream);
        }
    }
}

/// Aggregate statistics across multiple streams.
#[derive(Debug, Default, Clone)]
pub struct AggregateStats {
    /// Number of streams included in these stats.
    pub stream_count: usize,
    /// Total callbacks across all streams.
    pub total_callbacks: u64,
    /// Total underruns across all streams.
    pub total_underruns: u64,
    /// Total overruns across all streams.
    pub total_overruns: u64,
}

impl AggregateStats {
    /// Returns the average callbacks per stream.
    #[must_use]
    pub fn avg_callbacks(&self) -> f64 {
        if self.stream_count == 0 {
            0.0
        } else {
            self.total_callbacks as f64 / self.stream_count as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_stream::StreamConfig;
    use crate::callbacks::{InputCallbackContext, OutputCallbackContext};
    use crate::ring_buffer_pool::RingBufferPool;
    use crate::routing_table::RoutingTable;

    fn create_test_input_stream(device_id: &str) -> Arc<ActiveInputStream> {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let context = Arc::new(InputCallbackContext::new(
            Arc::clone(&pool),
            vec![],
            device_id,
        ));
        Arc::new(ActiveInputStream::new(
            format!("input-{device_id}"),
            device_id,
            StreamConfig::default(),
            vec![],
            context,
        ))
    }

    fn create_test_output_stream(device_id: &str) -> Arc<ActiveOutputStream> {
        let pool = Arc::new(RingBufferPool::new(8, 256));
        let routing = Arc::new(RoutingTable::new());
        let context = Arc::new(OutputCallbackContext::new(routing, pool, vec![], device_id));
        Arc::new(ActiveOutputStream::new(
            format!("output-{device_id}"),
            device_id,
            StreamConfig::default(),
            vec![],
            context,
        ))
    }

    #[test]
    fn new_registry_is_empty() {
        let registry = StreamRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.input_count(), 0);
        assert_eq!(registry.output_count(), 0);
        assert_eq!(registry.total_count(), 0);
    }

    #[test]
    fn register_input_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_input_stream("device-1");

        registry.register_input(stream);

        assert!(registry.has_input("device-1"));
        assert!(!registry.has_input("device-2"));
        assert_eq!(registry.input_count(), 1);
    }

    #[test]
    fn get_input_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_input_stream("device-1");

        registry.register_input(Arc::clone(&stream));

        let retrieved = registry.get_input("device-1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id(), stream.id());

        assert!(registry.get_input("nonexistent").is_none());
    }

    #[test]
    fn unregister_input_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_input_stream("device-1");

        registry.register_input(stream);
        assert!(registry.has_input("device-1"));

        let removed = registry.unregister_input("device-1");
        assert!(removed.is_some());
        assert!(!registry.has_input("device-1"));
        assert_eq!(registry.input_count(), 0);
    }

    #[test]
    fn register_output_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_output_stream("device-1");

        registry.register_output(stream);

        assert!(registry.has_output("device-1"));
        assert_eq!(registry.output_count(), 1);
    }

    #[test]
    fn get_output_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_output_stream("device-1");

        registry.register_output(Arc::clone(&stream));

        let retrieved = registry.get_output("device-1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id(), stream.id());
    }

    #[test]
    fn unregister_output_stream() {
        let registry = StreamRegistry::new();
        let stream = create_test_output_stream("device-1");

        registry.register_output(stream);
        assert!(registry.has_output("device-1"));

        let removed = registry.unregister_output("device-1");
        assert!(removed.is_some());
        assert!(!registry.has_output("device-1"));
    }

    #[test]
    fn multiple_streams() {
        let registry = StreamRegistry::new();

        registry.register_input(create_test_input_stream("input-1"));
        registry.register_input(create_test_input_stream("input-2"));
        registry.register_output(create_test_output_stream("output-1"));

        assert_eq!(registry.input_count(), 2);
        assert_eq!(registry.output_count(), 1);
        assert_eq!(registry.total_count(), 3);
        assert!(!registry.is_empty());
    }

    #[test]
    fn device_ids() {
        let registry = StreamRegistry::new();

        registry.register_input(create_test_input_stream("input-a"));
        registry.register_input(create_test_input_stream("input-b"));
        registry.register_output(create_test_output_stream("output-a"));

        let input_ids = registry.input_device_ids();
        assert_eq!(input_ids.len(), 2);
        assert!(input_ids.contains(&"input-a".to_string()));
        assert!(input_ids.contains(&"input-b".to_string()));

        let output_ids = registry.output_device_ids();
        assert_eq!(output_ids.len(), 1);
        assert!(output_ids.contains(&"output-a".to_string()));
    }

    #[test]
    fn clear_removes_all() {
        let registry = StreamRegistry::new();

        registry.register_input(create_test_input_stream("input-1"));
        registry.register_output(create_test_output_stream("output-1"));
        assert_eq!(registry.total_count(), 2);

        registry.clear();

        assert!(registry.is_empty());
        assert_eq!(registry.input_count(), 0);
        assert_eq!(registry.output_count(), 0);
    }

    #[test]
    fn replace_existing_stream() {
        let registry = StreamRegistry::new();

        let stream1 = create_test_input_stream("device-1");
        let stream2 = create_test_input_stream("device-1");

        registry.register_input(Arc::clone(&stream1));
        assert_eq!(registry.input_count(), 1);

        registry.register_input(Arc::clone(&stream2));
        assert_eq!(registry.input_count(), 1);

        // Should have the second stream
        let retrieved = registry.get_input("device-1").unwrap();
        assert_eq!(retrieved.id(), stream2.id());
    }

    #[test]
    fn for_each_input() {
        let registry = StreamRegistry::new();

        registry.register_input(create_test_input_stream("device-1"));
        registry.register_input(create_test_input_stream("device-2"));

        let mut count = 0;
        registry.for_each_input(|_stream| {
            count += 1;
        });

        assert_eq!(count, 2);
    }

    #[test]
    fn for_each_output() {
        let registry = StreamRegistry::new();

        registry.register_output(create_test_output_stream("device-1"));
        registry.register_output(create_test_output_stream("device-2"));
        registry.register_output(create_test_output_stream("device-3"));

        let mut count = 0;
        registry.for_each_output(|_stream| {
            count += 1;
        });

        assert_eq!(count, 3);
    }

    #[test]
    fn aggregate_stats() {
        let registry = StreamRegistry::new();

        let stream1 = create_test_input_stream("device-1");
        stream1.stats().record_callback();
        stream1.stats().record_callback();

        let stream2 = create_test_input_stream("device-2");
        stream2.stats().record_callback();
        stream2.stats().record_underrun();

        registry.register_input(stream1);
        registry.register_input(stream2);

        let stats = registry.input_stats();
        assert_eq!(stats.stream_count, 2);
        assert_eq!(stats.total_callbacks, 3);
        assert_eq!(stats.total_underruns, 1);
        assert_eq!(stats.total_overruns, 0);
        assert!((stats.avg_callbacks() - 1.5).abs() < 0.001);
    }

    #[test]
    fn aggregate_stats_empty() {
        let registry = StreamRegistry::new();

        let stats = registry.input_stats();
        assert_eq!(stats.stream_count, 0);
        assert_eq!(stats.total_callbacks, 0);
        assert!((stats.avg_callbacks() - 0.0).abs() < 0.001);
    }
}
