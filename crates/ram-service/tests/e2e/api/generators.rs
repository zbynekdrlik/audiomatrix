#![allow(clippy::doc_markdown)]
//! Generator endpoint E2E tests.

use ram_api::models::DeviceInfo;

use crate::e2e::TestClient;

/// Test listing generators for a device.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_list_generators() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an output device (generators are on output channels)
    if let Some(device) = devices.iter().find(|d| d.output_channels > 0) {
        let device_id = urlencoding::encode(&device.id);

        let gen_response = client
            .get(&format!("/nodes/LOCAL/devices/{device_id}/generators"))
            .await
            .expect("Failed to list generators");

        assert!(
            gen_response.status().is_success(),
            "List generators failed with status: {}",
            gen_response.status()
        );
    }
}

/// Test starting a sine wave generator.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_start_sine_generator() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an output device
    if let Some(device) = devices.iter().find(|d| d.output_channels > 0) {
        let device_id = urlencoding::encode(&device.id);

        let generator_request = serde_json::json!({
            "channel": 1,
            "waveform": "Sine",
            "frequency": 1000.0,
            "level_db": -20.0
        });

        let response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/generators"),
                &generator_request,
            )
            .await
            .expect("Failed to start generator");

        // Should succeed or conflict if already running
        assert!(
            response.status().is_success() || response.status().as_u16() == 409,
            "Start generator failed with unexpected status: {}",
            response.status()
        );

        // Stop the generator after test
        let _ = client
            .delete(&format!("/nodes/LOCAL/devices/{device_id}/generators/1"))
            .await;
    }
}

/// Test starting pink noise generator.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_start_pink_noise_generator() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an output device
    if let Some(device) = devices.iter().find(|d| d.output_channels > 0) {
        let device_id = urlencoding::encode(&device.id);

        let generator_request = serde_json::json!({
            "channel": 1,
            "waveform": "PinkNoise",
            "level_db": -30.0
        });

        let response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/generators"),
                &generator_request,
            )
            .await
            .expect("Failed to start generator");

        assert!(
            response.status().is_success() || response.status().as_u16() == 409,
            "Start pink noise generator failed with unexpected status: {}",
            response.status()
        );

        // Stop the generator after test
        let _ = client
            .delete(&format!("/nodes/LOCAL/devices/{device_id}/generators/1"))
            .await;
    }
}

/// Test stopping a generator.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_stop_generator() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an output device
    if let Some(device) = devices.iter().find(|d| d.output_channels > 0) {
        let device_id = urlencoding::encode(&device.id);

        // Start a generator first
        let generator_request = serde_json::json!({
            "channel": 2,
            "waveform": "Sine",
            "frequency": 440.0,
            "level_db": -40.0
        });

        let start_response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/generators"),
                &generator_request,
            )
            .await
            .expect("Failed to start generator");

        if start_response.status().is_success() {
            // Now stop it
            let stop_response = client
                .delete(&format!("/nodes/LOCAL/devices/{device_id}/generators/2"))
                .await
                .expect("Failed to stop generator");

            assert!(
                stop_response.status().is_success(),
                "Stop generator failed with status: {}",
                stop_response.status()
            );
        }
    }
}

/// Test starting generator with invalid channel returns 400.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_start_generator_invalid_channel() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an output device
    if let Some(device) = devices.iter().find(|d| d.output_channels > 0) {
        let device_id = urlencoding::encode(&device.id);

        // Try to start on channel 999 which shouldn't exist
        let generator_request = serde_json::json!({
            "channel": 999,
            "waveform": "Sine",
            "frequency": 1000.0,
            "level_db": -20.0
        });

        let response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/generators"),
                &generator_request,
            )
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status().as_u16(),
            400,
            "Invalid channel should return 400"
        );
    }
}

/// Test stopping nonexistent generator returns 404.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_stop_nonexistent_generator() {
    let client = TestClient::new();

    // Get devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);

        // Try to stop a generator that doesn't exist
        let response = client
            .delete(&format!("/nodes/LOCAL/devices/{device_id}/generators/999"))
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status().as_u16(),
            404,
            "Nonexistent generator should return 404"
        );
    }
}
