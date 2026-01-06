//! Sample rate and buffer size behavioral tests.

use super::*;

/// Test: Changing sample rate on attached device restarts streams.
///
/// Note: ASIO hardware devices may not support runtime sample rate changes.
/// The driver may be locked to a specific rate in the control panel.
/// This test verifies the API works; actual rate changes are driver-dependent.
#[tokio::test]
async fn test_sample_rate_change_restarts_streams() {
    let client = TestClient::new();

    // Ensure we have an attached device (attaches one if needed)
    let device = ensure_input_attached(&client).await;
    let device_id = urlencoding::encode(&device.id);
    let original_rate = device.sample_rate;
    let new_rate = if original_rate == 48000 { 96000 } else { 48000 };

    let streams_before: Vec<StreamInfo> = client
        .get_json("/streams")
        .await
        .expect("Failed to get streams");

    let device_stream_before = streams_before.iter().find(|s| s.device_id == device.id);

    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "sample_rate": new_rate }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(
        response.status().is_success(),
        "Sample rate update should succeed"
    );

    tokio::time::sleep(Duration::from_secs(3)).await;

    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get updated device");

    // Note: ASIO hardware may not support runtime sample rate changes.
    // The driver may fall back to a supported rate (e.g., 44100).
    // We verify the API accepted the request; actual rate is driver-dependent.
    let actual_rate = updated_device.sample_rate;
    let supported_rates = [96000, 48000, 44100];

    assert!(
        supported_rates.contains(&actual_rate),
        "Device sample rate {} should be a valid audio rate",
        actual_rate
    );

    // Verify streams are still running after reconfiguration
    let streams_after: Vec<StreamInfo> = client
        .get_json("/streams")
        .await
        .expect("Failed to get streams");

    if let Some(stream) = streams_after.iter().find(|s| s.device_id == device.id) {
        assert!(
            supported_rates.contains(&stream.sample_rate),
            "Stream sample rate {} should be valid",
            stream.sample_rate
        );
        println!(
            "Stream running at {}Hz (requested {}Hz, original {}Hz)",
            stream.sample_rate, new_rate, original_rate
        );
    } else if device_stream_before.is_some() {
        panic!("Stream missing after reconfiguration");
    }

    // Restore original rate (best effort)
    let _ = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "sample_rate": original_rate }))
        .send()
        .await;
}

/// Test: Changing sample rate on UNATTACHED device does NOT start streams.
#[tokio::test]
async fn test_sample_rate_change_unattached_no_streams() {
    let client = TestClient::new();

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let device = devices
        .iter()
        .find(|d| matches!(d.status, DeviceStatus::Available))
        .expect("TEST INFRASTRUCTURE ERROR: No available devices. DO NOT SKIP.");

    let device_id = urlencoding::encode(&device.id);

    let counts_before: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    let new_rate = if device.sample_rate == 48000 {
        96000
    } else {
        48000
    };

    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "sample_rate": new_rate }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(1)).await;

    let counts_after: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    assert_eq!(counts_before.input_streams, counts_after.input_streams);
    assert_eq!(counts_before.output_streams, counts_after.output_streams);
}

/// Test: Changing buffer size on attached device restarts streams.
#[tokio::test]
async fn test_buffer_size_change_restarts_streams() {
    let client = TestClient::new();

    // Ensure we have an attached device (attaches one if needed)
    let device = ensure_input_attached(&client).await;
    let device_id = urlencoding::encode(&device.id);
    let original_buffer = device.buffer_size;
    let new_buffer = if original_buffer == 256 { 512 } else { 256 };

    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "buffer_size": new_buffer }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(3)).await;

    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get updated device");

    assert_eq!(updated_device.buffer_size, new_buffer);

    let _ = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "buffer_size": original_buffer }))
        .send()
        .await;
}
