#![allow(clippy::doc_markdown)]
//! Device endpoint E2E tests.

use ram_api::models::{DeviceInfo, DeviceStatus, DeviceType};

use crate::e2e::TestClient;

/// Test listing devices for local node.
#[tokio::test]
async fn test_list_devices() {
    let client = TestClient::new();

    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    assert!(
        response.status().is_success(),
        "List devices failed with status: {}",
        response.status()
    );

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // May be empty if no audio devices, but should be valid response
    for device in &devices {
        assert!(!device.id.is_empty(), "Device should have ID");
        assert!(!device.name.is_empty(), "Device should have name");
        assert!(
            device.sample_rate > 0,
            "Device should have valid sample rate"
        );
    }
}

/// Test listing devices filters by type.
#[tokio::test]
async fn test_list_devices_by_type() {
    let client = TestClient::new();

    // Get all devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Verify device types are valid
    for device in &devices {
        match device.device_type {
            DeviceType::Input => {
                assert!(
                    device.input_channels > 0,
                    "Input device should have input channels"
                );
            },
            DeviceType::Output => {
                assert!(
                    device.output_channels > 0,
                    "Output device should have output channels"
                );
            },
            DeviceType::Duplex => {
                assert!(
                    device.input_channels > 0 || device.output_channels > 0,
                    "Duplex device should have channels"
                );
            },
        }
    }
}

/// Test getting a specific device by ID.
#[tokio::test]
async fn test_get_device_by_id() {
    let client = TestClient::new();

    // Get all devices first
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    if let Some(device) = devices.first() {
        // URL encode the device ID
        let device_id = urlencoding::encode(&device.id);
        let response = client
            .get(&format!("/nodes/LOCAL/devices/{device_id}"))
            .await
            .expect("Failed to get device");

        assert!(
            response.status().is_success(),
            "Get device failed with status: {}",
            response.status()
        );

        let fetched: DeviceInfo = response.json().await.expect("Failed to parse device");
        assert_eq!(fetched.id, device.id, "Device ID should match");
    }
}

/// Test getting nonexistent device returns 404.
#[tokio::test]
async fn test_get_nonexistent_device_returns_404() {
    let client = TestClient::new();

    let response = client
        .get("/nodes/LOCAL/devices/nonexistent-device-12345")
        .await
        .expect("Failed to get device");

    assert_eq!(
        response.status().as_u16(),
        404,
        "Nonexistent device should return 404"
    );
}

/// Test attaching a device.
#[tokio::test]
async fn test_attach_device() {
    let client = TestClient::new();

    // Get available devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an available device
    if let Some(available) = devices
        .iter()
        .find(|d| matches!(d.status, DeviceStatus::Available))
    {
        let device_id = urlencoding::encode(&available.id);

        let attach_request = serde_json::json!({
            "display_name": "Test Device"
        });

        let response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/attach"),
                &attach_request,
            )
            .await
            .expect("Failed to attach device");

        // Should succeed or already attached
        assert!(
            response.status().is_success() || response.status().as_u16() == 409,
            "Attach should succeed or return 409 if already attached: {}",
            response.status()
        );
    }
}

/// Test detaching a device.
#[tokio::test]
async fn test_detach_device() {
    let client = TestClient::new();

    // Get attached devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an attached device
    if let Some(attached) = devices.iter().find(|d| {
        matches!(d.status, DeviceStatus::Attached) || matches!(d.status, DeviceStatus::Active)
    }) {
        let device_id = urlencoding::encode(&attached.id);

        let response = client
            .post_json(
                &format!("/nodes/LOCAL/devices/{device_id}/detach"),
                &serde_json::json!({}),
            )
            .await
            .expect("Failed to detach device");

        // Should succeed or already detached
        assert!(
            response.status().is_success() || response.status().as_u16() == 409,
            "Detach should succeed or return 409 if not attached: {}",
            response.status()
        );
    }
}

/// Test getting device channels.
#[tokio::test]
async fn test_get_device_channels() {
    let client = TestClient::new();

    // Get all devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);

        let response = client
            .get(&format!("/nodes/LOCAL/devices/{device_id}/channels"))
            .await
            .expect("Failed to get channels");

        assert!(
            response.status().is_success(),
            "Get channels failed with status: {}",
            response.status()
        );
    }
}

/// Test updating device display name.
#[tokio::test]
async fn test_update_device_display_name() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find an attached device
    if let Some(device) = devices.iter().find(|d| {
        matches!(d.status, DeviceStatus::Attached) || matches!(d.status, DeviceStatus::Active)
    }) {
        let device_id = urlencoding::encode(&device.id);
        let new_name = "Test Display Name";

        let update_request = serde_json::json!({
            "display_name": new_name
        });

        // PATCH request
        let response = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to update device");

        assert!(
            response.status().is_success(),
            "Update device failed with status: {}",
            response.status()
        );

        let updated: DeviceInfo = response.json().await.expect("Failed to parse response");
        assert_eq!(
            updated.display_name.as_deref(),
            Some(new_name),
            "Display name should be updated"
        );
    }
}

/// Test updating device sample rate.
#[tokio::test]
async fn test_update_device_sample_rate() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find any device
    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);
        let original_rate = device.sample_rate;

        // Try to set to 48000 (a commonly supported rate)
        let new_rate = if original_rate == 48000 { 44100 } else { 48000 };

        let update_request = serde_json::json!({
            "sample_rate": new_rate
        });

        // PATCH request
        let response = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to update device");

        assert!(
            response.status().is_success(),
            "Update device sample rate failed with status: {}",
            response.status()
        );

        let updated: DeviceInfo = response.json().await.expect("Failed to parse response");
        assert_eq!(
            updated.sample_rate, new_rate,
            "Sample rate should be updated"
        );

        // Restore original rate
        let restore_request = serde_json::json!({
            "sample_rate": original_rate
        });
        let _ = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&restore_request)
            .send()
            .await;
    }
}

/// Test updating device buffer size.
#[tokio::test]
async fn test_update_device_buffer_size() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find any device
    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);
        let original_size = device.buffer_size;

        // Try to set to 256 (a commonly supported size)
        let new_size = if original_size == 256 { 512 } else { 256 };

        let update_request = serde_json::json!({
            "buffer_size": new_size
        });

        // PATCH request
        let response = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to update device");

        assert!(
            response.status().is_success(),
            "Update device buffer size failed with status: {}",
            response.status()
        );

        let updated: DeviceInfo = response.json().await.expect("Failed to parse response");
        assert_eq!(
            updated.buffer_size, new_size,
            "Buffer size should be updated"
        );

        // Restore original size
        let restore_request = serde_json::json!({
            "buffer_size": original_size
        });
        let _ = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&restore_request)
            .send()
            .await;
    }
}

/// Test updating device with invalid sample rate fails.
#[tokio::test]
async fn test_update_device_invalid_sample_rate() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find any device
    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);

        // Try to set an unsupported sample rate
        let update_request = serde_json::json!({
            "sample_rate": 12345  // Invalid rate
        });

        // PATCH request
        let response = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status().as_u16(),
            400,
            "Invalid sample rate should return 400 Bad Request"
        );
    }
}

/// Test updating device with invalid buffer size fails.
#[tokio::test]
async fn test_update_device_invalid_buffer_size() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find any device
    if let Some(device) = devices.first() {
        let device_id = urlencoding::encode(&device.id);

        // Try to set an unsupported buffer size
        let update_request = serde_json::json!({
            "buffer_size": 100  // Not a power of 2, invalid
        });

        // PATCH request
        let response = client
            .client
            .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status().as_u16(),
            400,
            "Invalid buffer size should return 400 Bad Request"
        );
    }
}

/// Test updating channel label.
#[tokio::test]
async fn test_update_channel_label() {
    let client = TestClient::new();

    // Get devices
    let response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let devices: Vec<DeviceInfo> = response.json().await.expect("Failed to parse devices");

    // Find a device with channels
    if let Some(device) = devices
        .iter()
        .find(|d| d.input_channels > 0 || d.output_channels > 0)
    {
        let device_id = urlencoding::encode(&device.id);
        let channel = 1;
        let new_label = "Test Channel Label";

        let update_request = serde_json::json!({
            "label": new_label
        });

        // PATCH request for channel label
        let response = client
            .client
            .patch(client.api_url(&format!(
                "/nodes/LOCAL/devices/{device_id}/channels/{channel}"
            )))
            .json(&update_request)
            .send()
            .await
            .expect("Failed to update channel label");

        assert!(
            response.status().is_success(),
            "Update channel label failed with status: {}",
            response.status()
        );

        // Verify the label was set
        let response = client
            .get(&format!("/nodes/LOCAL/devices/{device_id}/channels"))
            .await
            .expect("Failed to get channels");

        let channels: Vec<ram_api::models::ChannelInfo> =
            response.json().await.expect("Failed to parse channels");

        if let Some(ch) = channels.iter().find(|c| c.number == channel) {
            assert_eq!(
                ch.label.as_deref(),
                Some(new_label),
                "Channel label should be updated"
            );
        }

        // Clear the label
        let clear_request = serde_json::json!({
            "label": ""
        });
        let _ = client
            .client
            .patch(client.api_url(&format!(
                "/nodes/LOCAL/devices/{device_id}/channels/{channel}"
            )))
            .json(&clear_request)
            .send()
            .await;
    }
}
