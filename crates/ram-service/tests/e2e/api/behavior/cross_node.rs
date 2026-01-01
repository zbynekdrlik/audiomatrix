//! Cross-node routing behavioral tests.

use super::*;

/// Route response for parsing.
#[derive(Deserialize)]
struct RouteResponse {
    id: String,
}

/// Test: Cross-node route creation establishes network audio path.
#[tokio::test]
async fn test_cross_node_route_creation() {
    let client = TestClient::new();

    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to list nodes");

    let remote_node = nodes
        .iter()
        .find(|n| n.name != "LOCAL" && n.name != "local");

    let remote =
        remote_node.expect("TEST INFRASTRUCTURE ERROR: No remote nodes discovered. DO NOT SKIP.");

    let remote_devices: Vec<DeviceInfo> = client
        .get_json(&format!(
            "/nodes/{}/devices",
            urlencoding::encode(&remote.name)
        ))
        .await
        .expect("Failed to get remote devices");

    let remote_output = remote_devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Output | DeviceType::Duplex));

    let output_device = remote_output
        .expect("TEST INFRASTRUCTURE ERROR: No output devices on remote node. DO NOT SKIP.");

    let local_devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to get local devices");

    let local_input = local_devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let input_device = local_input
        .expect("TEST INFRASTRUCTURE ERROR: No attached local input devices. DO NOT SKIP.");

    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": input_device.id,
        "source_channel": 1,
        "destination_node": remote.name,
        "destination_device": output_device.id,
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create cross-node route");

    assert!(
        response.status().is_success(),
        "Cross-node route creation should succeed"
    );

    let route_response: RouteResponse = response.json().await.expect("Failed to parse response");

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let cross_node_route = routes
        .iter()
        .find(|r| r.get("destination_node").and_then(|v| v.as_str()) == Some(&remote.name));

    assert!(
        cross_node_route.is_some(),
        "Cross-node route should appear in route list"
    );

    let _ = client
        .delete(&format!("/routes/{}", route_response.id))
        .await;
}

/// Test: Bidirectional cross-node routing works.
#[tokio::test]
async fn test_cross_node_bidirectional_routing() {
    let client = TestClient::new();

    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to list nodes");

    let remote_node = nodes
        .iter()
        .find(|n| n.name != "LOCAL" && n.name != "local");

    let remote =
        remote_node.expect("TEST INFRASTRUCTURE ERROR: No remote nodes discovered. DO NOT SKIP.");

    let local_devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to get local devices");

    let remote_devices: Vec<DeviceInfo> = client
        .get_json(&format!(
            "/nodes/{}/devices",
            urlencoding::encode(&remote.name)
        ))
        .await
        .expect("Failed to get remote devices");

    let local_duplex = local_devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let remote_duplex = remote_devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Duplex));

    let local_dev =
        local_duplex.expect("TEST INFRASTRUCTURE ERROR: No local duplex device. DO NOT SKIP.");
    let remote_dev =
        remote_duplex.expect("TEST INFRASTRUCTURE ERROR: No remote duplex device. DO NOT SKIP.");

    // Create route: LOCAL -> REMOTE
    let route_out = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": local_dev.id,
        "source_channel": 1,
        "destination_node": remote.name,
        "destination_device": remote_dev.id,
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    let resp_out = client
        .post_json("/routes", &route_out)
        .await
        .expect("route out");
    assert!(
        resp_out.status().is_success(),
        "Outbound route should succeed"
    );

    let out_id: RouteResponse = resp_out.json().await.unwrap();

    // Create route: REMOTE -> LOCAL
    let route_in = serde_json::json!({
        "source_node": remote.name,
        "source_device": remote_dev.id,
        "source_channel": 2,
        "destination_node": "LOCAL",
        "destination_device": local_dev.id,
        "destination_channel": 2,
        "volume": 1.0,
        "muted": false
    });

    let resp_in = client
        .post_json("/routes", &route_in)
        .await
        .expect("route in");
    assert!(
        resp_in.status().is_success(),
        "Inbound route should succeed"
    );

    let in_id: RouteResponse = resp_in.json().await.unwrap();

    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    assert!(routes.len() >= 2, "Should have at least 2 routes");

    let _ = client.delete(&format!("/routes/{}", out_id.id)).await;
    let _ = client.delete(&format!("/routes/{}", in_id.id)).await;
}
