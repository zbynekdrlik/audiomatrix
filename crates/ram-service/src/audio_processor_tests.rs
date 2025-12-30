//! Tests for audio processor.

use super::*;

#[test]
fn audio_processor_creation() {
    let processor = AudioProcessor::with_defaults();
    assert!(!processor.is_running());
    assert_eq!(processor.buffer_pool().free_count(), 256);
}

#[test]
fn audio_processor_start_stop() {
    let processor = AudioProcessor::with_defaults();
    assert!(!processor.is_running());
    processor.start();
    assert!(processor.is_running());
    processor.stop();
    assert!(!processor.is_running());
}

#[test]
fn create_input_context() {
    let processor = AudioProcessor::with_defaults();
    let result = processor.create_input_context("device-1", 2);
    assert!(result.is_ok());

    let (context, indices) = result.unwrap();
    assert_eq!(context.channel_count(), 2);
    assert_eq!(indices.len(), 2);
    assert_eq!(processor.buffer_pool().allocated_count(), 2);
}

#[test]
fn create_output_context() {
    let processor = AudioProcessor::with_defaults();
    let (context, indices) = processor.create_output_context("device-1", 2);
    assert_eq!(context.dest_indices().len(), 2);
    assert_eq!(indices.len(), 2);

    let snapshot = processor.routing_table().snapshot();
    assert_eq!(snapshot.destinations.len(), 2);
}

#[test]
fn add_remove_route() {
    let processor = AudioProcessor::with_defaults();
    let conn_id = ConnectionId::new("LOCAL", "input-device", 1, "LOCAL", "output-device", 1);

    let initial_free_count = processor.buffer_pool().free_count();

    let buffer_idx = processor.add_route(conn_id.clone());
    assert!(buffer_idx.is_ok());

    let idx = buffer_idx.unwrap();
    assert!(processor.buffer_pool().is_allocated(idx));
    assert!(processor.get_connection_buffer(&conn_id).is_some());
    assert_eq!(processor.buffer_pool().free_count(), initial_free_count - 1);

    // Remove route - buffer is pending deferred free, not immediately freed
    let result = processor.remove_route(&conn_id);
    assert!(result.is_ok());
    // Buffer is still marked as allocated (pending deferred free)
    assert!(processor.buffer_pool().is_allocated(idx));
    assert_eq!(processor.buffer_pool().pending_free_count(), 1);

    // Bump generation and process pending frees to actually free the buffer
    for _ in 0..3 {
        processor
            .routing_table()
            .update_with(|current| current.clone());
    }
    let current_gen = processor.routing_table().generation();
    processor.buffer_pool().process_pending_frees(current_gen);

    // Now the buffer should be freed
    assert!(!processor.buffer_pool().is_allocated(idx));
    assert_eq!(processor.buffer_pool().free_count(), initial_free_count);
}

#[test]
fn stats() {
    let processor = AudioProcessor::with_defaults();
    let stats = processor.stats();
    assert!(!stats.is_running);
    assert_eq!(stats.free_buffers, 256);

    processor.start();
    let stats = processor.stats();
    assert!(stats.is_running);
}

#[test]
fn is_cross_node() {
    let processor = AudioProcessor::with_defaults();
    let local = ConnectionId::new("LOCAL", "in", 1, "LOCAL", "out", 1);
    let remote = ConnectionId::new("remote-node", "in", 1, "LOCAL", "out", 1);

    assert!(!processor.is_cross_node(&local));
    assert!(processor.is_cross_node(&remote));
}
