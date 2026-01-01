//! Route creation and modification behavioral tests.

use super::*;

/// Test: Creating a route actually establishes audio path.
#[tokio::test]
async fn test_route_creation_establishes_path() {
    let client = TestClient::new();

    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let input_device = devices.iter().find(|d| {
        (matches!(d.device_type, DeviceType::Input | DeviceType::Duplex))
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let output_device = devices.iter().find(|d| {
        (matches!(d.device_type, DeviceType::Output | DeviceType::Duplex))
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let input =
        input_device.expect("TEST INFRASTRUCTURE ERROR: No attached input devices. DO NOT SKIP.");
    let output =
        output_device.expect("TEST INFRASTRUCTURE ERROR: No attached output devices. DO NOT SKIP.");

    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": input.id,
        "source_channel": 1,
        "destination_node": "LOCAL",
        "destination_device": output.id,
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create route");

    assert!(
        response.status().is_success(),
        "Route creation should succeed"
    );

    #[derive(Deserialize)]
    struct RouteResponse {
        id: String,
    }
    let route_response: RouteResponse = response.json().await.expect("Failed to parse response");

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route_exists = routes
        .iter()
        .any(|r| r.get("source_device").and_then(|v| v.as_str()) == Some(&input.id));

    assert!(route_exists, "Created route should appear in route list");

    let _ = client
        .delete(&format!("/routes/{}", route_response.id))
        .await;
}

/// Test: Deleting a route removes the audio path.
#[tokio::test]
async fn test_route_deletion_removes_path() {
    let client = TestClient::new();

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route = routes
        .first()
        .expect("TEST INFRASTRUCTURE ERROR: No routes exist for deletion test. DO NOT SKIP.");

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    let response = client
        .delete(&format!("/routes/{}", urlencoding::encode(route_id)))
        .await
        .expect("Failed to delete route");

    assert!(
        response.status().is_success(),
        "Route deletion should succeed"
    );

    let routes_after: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route_still_exists = routes_after
        .iter()
        .any(|r| r.get("id").and_then(|v| v.as_str()) == Some(route_id));

    assert!(
        !route_still_exists,
        "Deleted route should not appear in route list"
    );
}

/// Test: Changing route volume actually affects the route.
#[tokio::test]
async fn test_route_volume_change_applied() {
    let client = TestClient::new();

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route = routes
        .first()
        .expect("TEST INFRASTRUCTURE ERROR: No routes exist. DO NOT SKIP.");

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    let response = client
        .client
        .patch(client.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .json(&serde_json::json!({ "volume": 0.5 }))
        .send()
        .await
        .expect("Failed to update route");

    assert!(
        response.status().is_success(),
        "Volume update should succeed"
    );

    let updated_route: serde_json::Value = client
        .get_json(&format!("/routes/{}", urlencoding::encode(route_id)))
        .await
        .expect("Failed to get route");

    let volume = updated_route
        .get("volume")
        .and_then(|v| v.as_f64())
        .expect("Route should have volume");

    assert!(
        (volume - 0.5).abs() < 0.01,
        "Route volume should be 0.5, got: {}",
        volume
    );
}

/// Test: Muting a route actually mutes it.
#[tokio::test]
async fn test_route_mute_applied() {
    let client = TestClient::new();

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route = routes
        .first()
        .expect("TEST INFRASTRUCTURE ERROR: No routes exist. DO NOT SKIP.");

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    let was_muted = route
        .get("muted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let response = client
        .client
        .patch(client.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .json(&serde_json::json!({ "muted": !was_muted }))
        .send()
        .await
        .expect("Failed to update route");

    assert!(response.status().is_success(), "Mute update should succeed");

    let updated_route: serde_json::Value = client
        .get_json(&format!("/routes/{}", urlencoding::encode(route_id)))
        .await
        .expect("Failed to get route");

    let is_muted = updated_route
        .get("muted")
        .and_then(|v| v.as_bool())
        .expect("Route should have muted");

    assert_eq!(is_muted, !was_muted, "Route mute state should have toggled");

    // Restore original state
    let _ = client
        .client
        .patch(client.api_url(&format!("/routes/{}", urlencoding::encode(route_id))))
        .json(&serde_json::json!({ "muted": was_muted }))
        .send()
        .await;
}
