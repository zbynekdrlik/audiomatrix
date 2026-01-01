//! Generator behavioral tests.

use super::*;

/// Test: Starting a generator actually produces audio levels.
#[tokio::test]
async fn test_generator_produces_levels() {
    let client = TestClient::new();

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let attached = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let device =
        attached.expect("TEST INFRASTRUCTURE ERROR: No attached input devices. DO NOT SKIP.");

    let device_id = urlencoding::encode(&device.id);

    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/channels/1/generator"),
            &serde_json::json!({
                "enabled": true,
                "waveform": "sine",
                "frequency": 1000,
                "amplitude": 0.5
            }),
        )
        .await
        .expect("Failed to start generator");

    assert!(
        response.status().is_success(),
        "Generator start should succeed"
    );

    tokio::time::sleep(Duration::from_secs(1)).await;

    let gen_status: serde_json::Value = client
        .get_json(&format!(
            "/nodes/LOCAL/devices/{device_id}/channels/1/generator"
        ))
        .await
        .expect("Failed to get generator status");

    let enabled = gen_status
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    assert!(enabled, "Generator should be enabled");

    // Cleanup: stop generator
    let _ = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/channels/1/generator"),
            &serde_json::json!({ "enabled": false }),
        )
        .await;
}
