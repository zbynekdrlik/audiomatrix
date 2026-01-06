//! Device attach/detach behavioral tests.

use super::*;

/// Test: After attaching a device, streams should actually start.
#[tokio::test]
async fn test_attach_device_starts_streams() {
    let client = TestClient::new();

    let initial_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get initial stream counts");

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let device = devices
        .iter()
        .find(|d| matches!(d.status, DeviceStatus::Available))
        .expect(
            "TEST INFRASTRUCTURE ERROR: No available devices. \
             Ensure stagebox1 has ASIO devices. DO NOT SKIP.",
        );

    let device_id = urlencoding::encode(&device.id);

    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/attach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to attach device");

    assert!(response.status().is_success(), "Attach should succeed");

    // Poll for stream count to increase (ASIO devices can take several seconds to start)
    let mut streams_increased = false;
    let (init_in, init_out) = (initial_counts.input_streams, initial_counts.output_streams);
    let mut final_counts = initial_counts;

    for attempt in 1..=10 {
        tokio::time::sleep(Duration::from_secs(1)).await;

        final_counts = client
            .get_json("/streams/count")
            .await
            .expect("Failed to get stream counts");

        streams_increased = match device.device_type {
            DeviceType::Input => final_counts.input_streams > init_in,
            DeviceType::Output => final_counts.output_streams > init_out,
            DeviceType::Duplex => {
                final_counts.input_streams > init_in || final_counts.output_streams > init_out
            },
        };

        if streams_increased {
            println!("Stream started after {} seconds", attempt);
            break;
        }

        if attempt % 3 == 0 {
            println!(
                "Attempt {}/10: in={}/{}, out={}/{}, waiting...",
                attempt, final_counts.input_streams, init_in, final_counts.output_streams, init_out
            );
        }
    }

    assert!(
        streams_increased,
        "Stream count should increase after attach (initial: in={}, out={}; final: in={}, out={})",
        init_in, init_out, final_counts.input_streams, final_counts.output_streams
    );

    let _ = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/detach"),
            &serde_json::json!({}),
        )
        .await;
}

/// Test: After detaching a device, streams should actually stop.
#[tokio::test]
async fn test_detach_device_stops_streams() {
    let client = TestClient::new();

    // Ensure we have an attached device to detach (attaches one if needed)
    let device = ensure_input_attached(&client).await;
    let device_id = urlencoding::encode(&device.id);

    let initial_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get initial stream counts");

    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/detach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to detach device");

    assert!(response.status().is_success(), "Detach should succeed");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let final_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get final stream counts");

    assert!(
        final_counts.input_streams < initial_counts.input_streams
            || final_counts.output_streams < initial_counts.output_streams,
        "Stream count should decrease after detach"
    );
}
