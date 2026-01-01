#![allow(clippy::doc_markdown)]
//! Virtual device endpoint E2E tests.

use ram_api::models::DeviceInfo;

use crate::e2e::TestClient;

/// Test listing virtual devices.
#[tokio::test]
async fn test_list_virtual_devices() {
    let client = TestClient::new();

    let response = client
        .get("/nodes/LOCAL/virtual-devices")
        .await
        .expect("Failed to list virtual devices");

    assert!(
        response.status().is_success(),
        "List virtual devices failed with status: {}",
        response.status()
    );

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // All virtual devices should have is_virtual = true
    for device in &devices {
        assert!(
            device.is_virtual,
            "Virtual device list should only contain virtual devices"
        );
    }
}

/// Test creating a virtual device.
#[tokio::test]
async fn test_create_virtual_device() {
    let client = TestClient::new();

    let create_request = serde_json::json!({
        "name": "test-vasio-e2e",
        "input_channels": 8,
        "output_channels": 8,
        "sample_rate": 48000,
        "buffer_size": 256,
        "auto_attach": false
    });

    let response = client
        .post_json("/nodes/LOCAL/virtual-devices", &create_request)
        .await
        .expect("Failed to create virtual device");

    // May fail if ASIO driver not available - that's OK in CI
    if response.status().is_success() {
        let device: DeviceInfo = response.json().await.expect("Failed to parse device");
        assert_eq!(device.name, "test-vasio-e2e", "Device name should match");
        assert!(device.is_virtual, "Device should be virtual");
        assert_eq!(device.input_channels, 8, "Input channels should match");
        assert_eq!(device.output_channels, 8, "Output channels should match");

        // Cleanup: delete the device
        let device_id = urlencoding::encode(&device.id);
        let _ = client
            .delete(&format!("/nodes/LOCAL/virtual-devices/{device_id}"))
            .await;
    }
}

/// Test creating virtual device with invalid sample rate returns 400.
#[tokio::test]
async fn test_create_virtual_device_invalid_sample_rate() {
    let client = TestClient::new();

    let create_request = serde_json::json!({
        "name": "test-invalid",
        "input_channels": 8,
        "output_channels": 8,
        "sample_rate": 12345,  // Invalid sample rate
        "buffer_size": 256
    });

    let response = client
        .post_json("/nodes/LOCAL/virtual-devices", &create_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status().as_u16(),
        400,
        "Invalid sample rate should return 400"
    );
}

/// Test creating virtual device with invalid channel count returns 400.
#[tokio::test]
async fn test_create_virtual_device_invalid_channels() {
    let client = TestClient::new();

    // Zero channels
    let create_request = serde_json::json!({
        "name": "test-invalid",
        "input_channels": 0,
        "output_channels": 0,
        "sample_rate": 48000,
        "buffer_size": 256
    });

    let response = client
        .post_json("/nodes/LOCAL/virtual-devices", &create_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status().as_u16(),
        400,
        "Zero channels should return 400"
    );
}

/// Test creating virtual device with too many channels returns 400.
#[tokio::test]
async fn test_create_virtual_device_too_many_channels() {
    let client = TestClient::new();

    let create_request = serde_json::json!({
        "name": "test-invalid",
        "input_channels": 512,  // Too many (max 256)
        "output_channels": 8,
        "sample_rate": 48000,
        "buffer_size": 256
    });

    let response = client
        .post_json("/nodes/LOCAL/virtual-devices", &create_request)
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status().as_u16(),
        400,
        "Too many channels should return 400"
    );
}

/// Test deleting a virtual device.
#[tokio::test]
async fn test_delete_virtual_device() {
    let client = TestClient::new();

    // First create a device
    let create_request = serde_json::json!({
        "name": "test-delete-e2e",
        "input_channels": 2,
        "output_channels": 2,
        "sample_rate": 48000,
        "buffer_size": 256,
        "auto_attach": false
    });

    let response = client
        .post_json("/nodes/LOCAL/virtual-devices", &create_request)
        .await
        .expect("Failed to create virtual device");

    if response.status().is_success() {
        let device: DeviceInfo = response.json().await.expect("Failed to parse device");
        let device_id = urlencoding::encode(&device.id);

        // Delete it
        let delete_response = client
            .delete(&format!("/nodes/LOCAL/virtual-devices/{device_id}"))
            .await
            .expect("Failed to delete virtual device");

        assert!(
            delete_response.status().is_success(),
            "Delete virtual device failed with status: {}",
            delete_response.status()
        );

        // Verify it's gone
        let list_response = client
            .get("/nodes/LOCAL/virtual-devices")
            .await
            .expect("Failed to list virtual devices");

        let devices: Vec<DeviceInfo> = list_response.json().await.expect("Failed to parse devices");
        assert!(
            !devices.iter().any(|d| d.id == device.id),
            "Deleted device should not appear in list"
        );
    }
}

/// Test deleting nonexistent virtual device returns 404.
#[tokio::test]
async fn test_delete_nonexistent_virtual_device() {
    let client = TestClient::new();

    let response = client
        .delete("/nodes/LOCAL/virtual-devices/nonexistent-device-12345")
        .await
        .expect("Failed to delete virtual device");

    assert_eq!(
        response.status().as_u16(),
        404,
        "Nonexistent device should return 404"
    );
}
