//! Persistence behavioral tests.

use super::*;

/// Test: Device configuration persists across API calls.
#[tokio::test]
async fn test_device_config_persists() {
    let client = TestClient::new();

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let device = devices
        .first()
        .expect("TEST INFRASTRUCTURE ERROR: No devices on test server. DO NOT SKIP.");

    let device_id = urlencoding::encode(&device.id);
    let original_name = device
        .display_name
        .clone()
        .unwrap_or_else(|| device.id.clone());

    let new_name = format!("{} (Test)", original_name);
    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "display_name": new_name }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(response.status().is_success());

    let updated: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get device");

    assert_eq!(
        updated.display_name.as_deref(),
        Some(new_name.as_str()),
        "Display name should persist"
    );

    // Restore original name
    let _ = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "display_name": original_name }))
        .send()
        .await;
}

/// Test: Routes persist and are restored correctly.
#[tokio::test]
async fn test_route_persists() {
    let client = TestClient::new();

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let input = devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Input | DeviceType::Duplex));
    let output = devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Output | DeviceType::Duplex));

    let inp = input.expect("TEST INFRASTRUCTURE ERROR: No input devices. DO NOT SKIP.");
    let out = output.expect("TEST INFRASTRUCTURE ERROR: No output devices. DO NOT SKIP.");

    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": inp.id,
        "source_channel": 7,
        "destination_node": "LOCAL",
        "destination_device": out.id,
        "destination_channel": 8,
        "volume": 0.77,
        "muted": false
    });

    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create route");

    assert!(response.status().is_success());

    #[derive(Deserialize)]
    struct RouteResp {
        id: String,
    }
    let created: RouteResp = response.json().await.unwrap();

    let stored_route: serde_json::Value = client
        .get_json(&format!("/routes/{}", urlencoding::encode(&created.id)))
        .await
        .expect("Failed to get route");

    let volume = stored_route
        .get("volume")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    assert!(
        (volume - 0.77).abs() < 0.01,
        "Route volume should persist as 0.77, got: {}",
        volume
    );

    let src_channel = stored_route
        .get("source_channel")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    assert_eq!(src_channel, 7, "Source channel should persist as 7");

    let _ = client.delete(&format!("/routes/{}", created.id)).await;
}
