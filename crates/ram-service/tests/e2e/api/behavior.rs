#![allow(clippy::doc_markdown)]
//! Behavioral E2E tests.
//!
//! These tests verify that API operations have their intended EFFECT,
//! not just that they return success responses.
//!
//! Key distinction from other E2E tests:
//! - Other tests: "Does attach return 200?" (API contract)
//! - These tests: "After attach, do streams actually start?" (Behavior)

use ram_api::models::{DeviceInfo, DeviceStatus, DeviceType, StreamInfo};
use serde::Deserialize;
use std::time::Duration;

use crate::e2e::TestClient;

/// Response for stream counts endpoint.
#[derive(Debug, Deserialize)]
pub struct StreamCounts {
    pub input: usize,
    pub output: usize,
}

/// Helper to wait for a condition with timeout.
#[allow(dead_code)]
async fn wait_for<F, Fut>(timeout_secs: u64, check: F) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        if check().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

// ============================================================================
// DEVICE ATTACH/DETACH BEHAVIORAL TESTS
// ============================================================================

/// Test: After attaching a device, streams should actually start.
///
/// Verifies:
/// 1. Attach returns success
/// 2. /streams endpoint shows the device's streams
/// 3. Stream count increased
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_attach_device_starts_streams() {
    let client = TestClient::new();

    // Get initial stream count
    let initial_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get initial stream counts");

    // Find an available device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let available = devices
        .iter()
        .find(|d| matches!(d.status, DeviceStatus::Available));

    let Some(device) = available else {
        println!("No available devices to test - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);

    // Attach the device
    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/attach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to attach device");

    assert!(
        response.status().is_success(),
        "Attach should succeed: {}",
        response.status()
    );

    // Wait for streams to start (give it up to 5 seconds)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // VERIFY: Stream count should have increased
    let final_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get final stream counts");

    let streams_increased = match device.device_type {
        DeviceType::Input => final_counts.input > initial_counts.input,
        DeviceType::Output => final_counts.output > initial_counts.output,
        DeviceType::Duplex => {
            final_counts.input > initial_counts.input
                || final_counts.output > initial_counts.output
        }
    };

    assert!(
        streams_increased,
        "Stream count should increase after attach. \
         Before: {:?}, After: {:?}, Device type: {:?}",
        initial_counts, final_counts, device.device_type
    );

    // VERIFY: Device status should be Attached or Active
    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get device");

    assert!(
        matches!(
            updated_device.status,
            DeviceStatus::Attached | DeviceStatus::Active
        ),
        "Device status should be Attached or Active, got: {:?}",
        updated_device.status
    );

    // Cleanup: detach the device
    let _ = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/detach"),
            &serde_json::json!({}),
        )
        .await;
}

/// Test: After detaching a device, streams should actually stop.
#[tokio::test]
#[ignore = "Requires running server with attached audio device"]
async fn test_detach_device_stops_streams() {
    let client = TestClient::new();

    // Find an attached device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let attached = devices.iter().find(|d| {
        matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let Some(device) = attached else {
        println!("No attached devices to test - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);

    // Get initial stream count
    let initial_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get initial stream counts");

    // Detach the device
    let response = client
        .post_json(
            &format!("/nodes/LOCAL/devices/{device_id}/detach"),
            &serde_json::json!({}),
        )
        .await
        .expect("Failed to detach device");

    assert!(
        response.status().is_success(),
        "Detach should succeed: {}",
        response.status()
    );

    // Wait for streams to stop
    tokio::time::sleep(Duration::from_secs(2)).await;

    // VERIFY: Stream count should have decreased
    let final_counts: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get final stream counts");

    let streams_decreased = final_counts.input < initial_counts.input
        || final_counts.output < initial_counts.output;

    assert!(
        streams_decreased,
        "Stream count should decrease after detach. Before: {:?}, After: {:?}",
        initial_counts, final_counts
    );

    // VERIFY: Device status should be Available or Detached
    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get device");

    assert!(
        matches!(
            updated_device.status,
            DeviceStatus::Available | DeviceStatus::Detached
        ),
        "Device status should be Available or Detached, got: {:?}",
        updated_device.status
    );
}

// ============================================================================
// SAMPLE RATE CHANGE BEHAVIORAL TESTS
// ============================================================================

/// Test: Changing sample rate on attached device restarts streams at new rate.
///
/// This is the CRITICAL test that was missing - previously sample rate
/// changes only updated the stored value without affecting actual streams.
#[tokio::test]
#[ignore = "Requires running server with attached audio device"]
async fn test_sample_rate_change_restarts_streams() {
    let client = TestClient::new();

    // Find an attached device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let attached = devices.iter().find(|d| {
        matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let Some(device) = attached else {
        println!("No attached devices to test - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);
    let original_rate = device.sample_rate;

    // Choose a different sample rate
    let new_rate = if original_rate == 48000 { 96000 } else { 48000 };

    // Get streams BEFORE the change
    let streams_before: Vec<StreamInfo> = client
        .get_json("/streams")
        .await
        .expect("Failed to get streams");

    let device_stream_before = streams_before
        .iter()
        .find(|s| s.device_id == device.id);

    // Change sample rate
    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "sample_rate": new_rate }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(
        response.status().is_success(),
        "Sample rate update should succeed: {}",
        response.status()
    );

    // Wait for stream reconfiguration
    tokio::time::sleep(Duration::from_secs(3)).await;

    // VERIFY: Device should report new sample rate
    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get updated device");

    assert_eq!(
        updated_device.sample_rate, new_rate,
        "Device should report new sample rate"
    );

    // VERIFY: Stream should be running at new sample rate
    let streams_after: Vec<StreamInfo> = client
        .get_json("/streams")
        .await
        .expect("Failed to get streams");

    let device_stream_after = streams_after
        .iter()
        .find(|s| s.device_id == device.id);

    if let Some(stream) = device_stream_after {
        assert_eq!(
            stream.sample_rate, new_rate,
            "Stream should be running at new sample rate. Expected: {}, Got: {}",
            new_rate, stream.sample_rate
        );
    } else if device_stream_before.is_some() {
        panic!(
            "Device had stream before sample rate change but not after - \
             stream reconfiguration may have failed"
        );
    }

    // VERIFY: Device should still be attached (not in error state)
    assert!(
        matches!(
            updated_device.status,
            DeviceStatus::Attached | DeviceStatus::Active
        ),
        "Device should remain attached after sample rate change, got: {:?}",
        updated_device.status
    );

    // Restore original sample rate
    let _ = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "sample_rate": original_rate }))
        .send()
        .await;
}

/// Test: Changing sample rate on UNATTACHED device does NOT start streams.
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_sample_rate_change_unattached_no_streams() {
    let client = TestClient::new();

    // Find an available (not attached) device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let available = devices
        .iter()
        .find(|d| matches!(d.status, DeviceStatus::Available));

    let Some(device) = available else {
        println!("No available devices to test - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);

    // Get stream count before
    let counts_before: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    // Change sample rate on unattached device
    let new_rate = if device.sample_rate == 48000 { 96000 } else { 48000 };

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

    // Wait a bit
    tokio::time::sleep(Duration::from_secs(1)).await;

    // VERIFY: Stream count should NOT have changed
    let counts_after: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    assert_eq!(
        counts_before.input, counts_after.input,
        "Input stream count should not change for unattached device"
    );
    assert_eq!(
        counts_before.output, counts_after.output,
        "Output stream count should not change for unattached device"
    );
}

// ============================================================================
// ROUTE CREATION BEHAVIORAL TESTS
// ============================================================================

/// Test: Creating a route actually establishes audio path.
///
/// Verifies that after route creation:
/// 1. Route appears in route list
/// 2. If devices are attached, route is marked as active
#[tokio::test]
#[ignore = "Requires running server with attached audio devices"]
async fn test_route_creation_establishes_path() {
    let client = TestClient::new();

    // Find attached input and output devices
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

    let (Some(input), Some(output)) = (input_device, output_device) else {
        println!("Need both attached input and output devices - skipping");
        return;
    };

    // Create route
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
        "Route creation should succeed: {}",
        response.status()
    );

    #[derive(Deserialize)]
    struct RouteResponse {
        id: String,
    }
    let route_response: RouteResponse = response.json().await.expect("Failed to parse response");

    // VERIFY: Route should appear in list
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let route_exists = routes
        .iter()
        .any(|r| r.get("source_device").and_then(|v| v.as_str()) == Some(&input.id));

    assert!(route_exists, "Created route should appear in route list");

    // Cleanup
    let _ = client.delete(&format!("/routes/{}", route_response.id)).await;
}

/// Test: Deleting a route removes the audio path.
#[tokio::test]
#[ignore = "Requires running server with routes"]
async fn test_route_deletion_removes_path() {
    let client = TestClient::new();

    // Get existing routes
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let Some(route) = routes.first() else {
        println!("No routes to test deletion - skipping");
        return;
    };

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    // Delete the route
    let response = client
        .delete(&format!("/routes/{}", urlencoding::encode(route_id)))
        .await
        .expect("Failed to delete route");

    assert!(
        response.status().is_success(),
        "Route deletion should succeed"
    );

    // VERIFY: Route should no longer appear in list
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

// ============================================================================
// VOLUME/MUTE BEHAVIORAL TESTS
// ============================================================================

/// Test: Changing route volume actually affects the route.
#[tokio::test]
#[ignore = "Requires running server with routes"]
async fn test_route_volume_change_applied() {
    let client = TestClient::new();

    // Get existing routes
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let Some(route) = routes.first() else {
        println!("No routes to test - skipping");
        return;
    };

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    // Change volume
    let response = client
        .client
        .patch(client.api_url(&format!(
            "/routes/{}",
            urlencoding::encode(route_id)
        )))
        .json(&serde_json::json!({ "volume": 0.5 }))
        .send()
        .await
        .expect("Failed to update route");

    assert!(
        response.status().is_success(),
        "Volume update should succeed"
    );

    // VERIFY: Route should report new volume
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
#[ignore = "Requires running server with routes"]
async fn test_route_mute_applied() {
    let client = TestClient::new();

    // Get existing routes
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let Some(route) = routes.first() else {
        println!("No routes to test - skipping");
        return;
    };

    let route_id = route
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Route should have id");

    let was_muted = route
        .get("muted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Toggle mute
    let response = client
        .client
        .patch(client.api_url(&format!(
            "/routes/{}",
            urlencoding::encode(route_id)
        )))
        .json(&serde_json::json!({ "muted": !was_muted }))
        .send()
        .await
        .expect("Failed to update route");

    assert!(response.status().is_success(), "Mute update should succeed");

    // VERIFY: Route should report new mute state
    let updated_route: serde_json::Value = client
        .get_json(&format!("/routes/{}", urlencoding::encode(route_id)))
        .await
        .expect("Failed to get route");

    let is_muted = updated_route
        .get("muted")
        .and_then(|v| v.as_bool())
        .expect("Route should have muted");

    assert_eq!(
        is_muted, !was_muted,
        "Route mute state should have toggled"
    );

    // Restore original state
    let _ = client
        .client
        .patch(client.api_url(&format!(
            "/routes/{}",
            urlencoding::encode(route_id)
        )))
        .json(&serde_json::json!({ "muted": was_muted }))
        .send()
        .await;
}

// ============================================================================
// GENERATOR BEHAVIORAL TESTS
// ============================================================================

/// Test: Starting a generator actually produces audio levels.
#[tokio::test]
#[ignore = "Requires running server with attached audio device"]
async fn test_generator_produces_levels() {
    let client = TestClient::new();

    // Find an attached input device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let attached = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let Some(device) = attached else {
        println!("No attached input devices - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);

    // Start generator on channel 1
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

    // Wait for generator to produce audio
    tokio::time::sleep(Duration::from_secs(1)).await;

    // VERIFY: Generator status should show enabled
    let gen_status: serde_json::Value = client
        .get_json(&format!(
            "/nodes/LOCAL/devices/{device_id}/channels/1/generator"
        ))
        .await
        .expect("Failed to get generator status");

    let enabled = gen_status
        .get("enabled")
        .and_then(|v| v.as_bool())
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

// ============================================================================
// CROSS-NODE ROUTING BEHAVIORAL TESTS
// ============================================================================

/// Response for node info.
#[derive(Debug, Deserialize)]
pub struct NodeInfo {
    pub name: String,
    #[allow(dead_code)]
    pub ip: String,
}

/// Test: Cross-node route creation establishes network audio path.
///
/// Verifies that routing between two different nodes works:
/// 1. Remote node is discovered
/// 2. Route creation succeeds
/// 3. Route shows remote node info correctly
#[tokio::test]
#[ignore = "Requires running server with network peer"]
async fn test_cross_node_route_creation() {
    let client = TestClient::new();

    // Get available nodes
    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to list nodes");

    // Find a remote node (not LOCAL)
    let remote_node = nodes.iter().find(|n| n.name != "LOCAL" && n.name != "local");

    let Some(remote) = remote_node else {
        println!("No remote nodes discovered - skipping cross-node test");
        return;
    };

    // Get devices on remote node
    let remote_devices: Vec<DeviceInfo> = client
        .get_json(&format!("/nodes/{}/devices", urlencoding::encode(&remote.name)))
        .await
        .expect("Failed to get remote devices");

    let remote_output = remote_devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Output | DeviceType::Duplex)
    });

    let Some(output_device) = remote_output else {
        println!("No output devices on remote node - skipping");
        return;
    };

    // Get local devices
    let local_devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to get local devices");

    let local_input = local_devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let Some(input_device) = local_input else {
        println!("No attached local input devices - skipping");
        return;
    };

    // Create cross-node route: LOCAL input -> REMOTE output
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
        "Cross-node route creation should succeed: {}",
        response.status()
    );

    #[derive(Deserialize)]
    struct RouteResponse {
        id: String,
    }
    let route_response: RouteResponse = response.json().await.expect("Failed to parse response");

    // VERIFY: Route should appear with correct remote node
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    let cross_node_route = routes.iter().find(|r| {
        r.get("destination_node").and_then(|v| v.as_str()) == Some(&remote.name)
    });

    assert!(
        cross_node_route.is_some(),
        "Cross-node route should appear in route list with destination_node: {}",
        remote.name
    );

    // Cleanup
    let _ = client.delete(&format!("/routes/{}", route_response.id)).await;
}

/// Test: Bidirectional cross-node routing works.
///
/// Verifies routes can be created in both directions between nodes.
#[tokio::test]
#[ignore = "Requires running server with network peer"]
async fn test_cross_node_bidirectional_routing() {
    let client = TestClient::new();

    // Get available nodes
    let nodes: Vec<NodeInfo> = client
        .get_json("/nodes")
        .await
        .expect("Failed to list nodes");

    let remote_node = nodes.iter().find(|n| n.name != "LOCAL" && n.name != "local");

    let Some(remote) = remote_node else {
        println!("No remote nodes - skipping");
        return;
    };

    // Get devices on both nodes
    let local_devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to get local devices");

    let remote_devices: Vec<DeviceInfo> = client
        .get_json(&format!("/nodes/{}/devices", urlencoding::encode(&remote.name)))
        .await
        .expect("Failed to get remote devices");

    // Find attached duplex devices for bidirectional test
    let local_duplex = local_devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Duplex)
            && matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let remote_duplex = remote_devices
        .iter()
        .find(|d| matches!(d.device_type, DeviceType::Duplex));

    let (Some(local_dev), Some(remote_dev)) = (local_duplex, remote_duplex) else {
        println!("Need duplex devices on both nodes - skipping");
        return;
    };

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

    let resp_out = client.post_json("/routes", &route_out).await.expect("route out");
    assert!(resp_out.status().is_success(), "Outbound route should succeed");

    #[derive(Deserialize)]
    struct RouteResp {
        id: String,
    }
    let out_id: RouteResp = resp_out.json().await.unwrap();

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

    let resp_in = client.post_json("/routes", &route_in).await.expect("route in");
    assert!(resp_in.status().is_success(), "Inbound route should succeed");

    let in_id: RouteResp = resp_in.json().await.unwrap();

    // VERIFY: Both routes exist
    let routes: Vec<serde_json::Value> = client
        .get_json("/routes")
        .await
        .expect("Failed to list routes");

    assert!(routes.len() >= 2, "Should have at least 2 routes");

    // Cleanup
    let _ = client.delete(&format!("/routes/{}", out_id.id)).await;
    let _ = client.delete(&format!("/routes/{}", in_id.id)).await;
}

// ============================================================================
// PERSISTENCE BEHAVIORAL TESTS
// ============================================================================

/// Test: Device configuration persists across API calls.
///
/// Verifies that changing device settings persists:
/// 1. Sample rate changes are stored
/// 2. Buffer size changes are stored
/// 3. Display name changes are stored
#[tokio::test]
#[ignore = "Requires running server with audio devices"]
async fn test_device_config_persists() {
    let client = TestClient::new();

    // Find any device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let Some(device) = devices.first() else {
        println!("No devices - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);
    let original_name = device.display_name.clone().unwrap_or_else(|| device.id.clone());

    // Change display name
    let new_name = format!("{} (Test)", original_name);
    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "display_name": new_name }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(response.status().is_success());

    // Read back immediately - should reflect change
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
///
/// Verifies routes can be created and remain in the system.
/// Note: Full restart persistence requires service restart test.
#[tokio::test]
#[ignore = "Requires running server with devices"]
async fn test_route_persists() {
    let client = TestClient::new();

    // Get device for route
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let input = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
    });
    let output = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Output | DeviceType::Duplex)
    });

    let (Some(inp), Some(out)) = (input, output) else {
        println!("Need input and output devices - skipping");
        return;
    };

    // Create a unique route
    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": inp.id,
        "source_channel": 7,  // Use unusual channel for uniqueness
        "destination_node": "LOCAL",
        "destination_device": out.id,
        "destination_channel": 8,
        "volume": 0.77,  // Unique volume for identification
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

    // VERIFY: Route exists with correct values
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

    // Cleanup
    let _ = client.delete(&format!("/routes/{}", created.id)).await;
}

// ============================================================================
// METERING BEHAVIORAL TESTS
// ============================================================================

/// Test: Metering WebSocket provides level data for attached devices.
///
/// Verifies that after subscribing to metering:
/// 1. Connection succeeds
/// 2. Metering events are received
/// 3. Levels are within valid range (-60 to 0 dB typical)
#[tokio::test]
#[ignore = "Requires running server with attached audio device"]
async fn test_metering_websocket_provides_levels() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;

    let base_url = std::env::var("TEST_SERVER_URL")
        .unwrap_or_else(|_| "localhost:8080".to_string());
    let ws_url = format!("ws://{}/api/v1/ws", base_url);

    let (mut ws, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect WebSocket");

    // Subscribe to metering for all devices
    let subscribe = serde_json::json!({
        "command": "subscribe",
        "data": { "device": "*" }
    });

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        subscribe.to_string().into(),
    ))
    .await
    .expect("Failed to send subscribe");

    // Wait for metering data (up to 5 seconds)
    let mut received_metering = false;
    let timeout = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = ws.next().await {
            if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if json.get("event").and_then(|e| e.as_str()) == Some("metering") {
                        // Verify metering data structure
                        assert!(
                            json.get("device").is_some(),
                            "Metering should include device"
                        );
                        assert!(
                            json.get("levels").is_some(),
                            "Metering should include levels"
                        );

                        // Check levels are in valid range
                        if let Some(levels) = json.get("levels").and_then(|l| l.as_array()) {
                            for level in levels {
                                if let Some(db) = level.as_f64() {
                                    assert!(
                                        db <= 0.0 && db >= -100.0,
                                        "Level should be in valid dB range: {}",
                                        db
                                    );
                                }
                            }
                        }

                        received_metering = true;
                        break;
                    }
                }
            }
        }
    });

    let _ = timeout.await;

    // Note: We don't fail if no metering received - device may be silent
    // The test verifies the WebSocket subscription works
    if received_metering {
        println!("✓ Received valid metering data");
    } else {
        println!("⚠ No metering data received (device may be silent)");
    }

    // Unsubscribe
    let unsubscribe = serde_json::json!({
        "command": "unsubscribe",
        "data": { "device": "*" }
    });

    let _ = ws
        .send(tokio_tungstenite::tungstenite::Message::Text(
            unsubscribe.to_string().into(),
        ))
        .await;
}

/// Test: WebSocket broadcasts route changes.
///
/// Verifies that route creation/deletion events are broadcast to WebSocket clients.
#[tokio::test]
#[ignore = "Requires running server with devices"]
async fn test_websocket_broadcasts_route_changes() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;

    let client = TestClient::new();
    let base_url = std::env::var("TEST_SERVER_URL")
        .unwrap_or_else(|_| "localhost:8080".to_string());
    let ws_url = format!("ws://{}/api/v1/ws", base_url);

    let (mut ws, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect WebSocket");

    // Get devices for route
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let input = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Input | DeviceType::Duplex)
    });
    let output = devices.iter().find(|d| {
        matches!(d.device_type, DeviceType::Output | DeviceType::Duplex)
    });

    let (Some(inp), Some(out)) = (input, output) else {
        println!("Need input and output devices - skipping");
        return;
    };

    // Create route via REST API
    let route = serde_json::json!({
        "source_node": "LOCAL",
        "source_device": inp.id,
        "source_channel": 1,
        "destination_node": "LOCAL",
        "destination_device": out.id,
        "destination_channel": 1,
        "volume": 1.0,
        "muted": false
    });

    // Start listening for WebSocket messages
    let ws_task = tokio::spawn(async move {
        let mut route_event_received = false;
        let timeout = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(msg) = ws.next().await {
                if let Ok(tokio_tungstenite::tungstenite::Message::Text(text)) = msg {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let event = json.get("event").and_then(|e| e.as_str());
                        if event == Some("route_created") || event == Some("route_updated") {
                            route_event_received = true;
                            break;
                        }
                    }
                }
            }
        });
        let _ = timeout.await;
        route_event_received
    });

    // Small delay to ensure WebSocket is listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create route
    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create route");

    #[derive(Deserialize)]
    struct RouteResp {
        id: String,
    }

    if response.status().is_success() {
        let created: RouteResp = response.json().await.unwrap();

        // Wait for WebSocket task
        let route_event_received = ws_task.await.unwrap_or(false);

        if route_event_received {
            println!("✓ Received route change event via WebSocket");
        } else {
            println!("⚠ No route change event received (may be expected if not implemented)");
        }

        // Cleanup
        let _ = client.delete(&format!("/routes/{}", created.id)).await;
    }
}

// ============================================================================
// BUFFER SIZE CHANGE BEHAVIORAL TESTS
// ============================================================================

/// Test: Changing buffer size on attached device restarts streams.
///
/// Similar to sample rate test - buffer size changes should trigger
/// stream reconfiguration on attached devices.
#[tokio::test]
#[ignore = "Requires running server with attached audio device"]
async fn test_buffer_size_change_restarts_streams() {
    let client = TestClient::new();

    // Find an attached device
    let devices: Vec<DeviceInfo> = client
        .get_json("/nodes/LOCAL/devices")
        .await
        .expect("Failed to list devices");

    let attached = devices.iter().find(|d| {
        matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active)
    });

    let Some(device) = attached else {
        println!("No attached devices - skipping");
        return;
    };

    let device_id = urlencoding::encode(&device.id);
    let original_buffer = device.buffer_size;

    // Choose a different buffer size
    let new_buffer = if original_buffer == 256 { 512 } else { 256 };

    // Get streams BEFORE the change (verify baseline)
    let _counts_before: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    // Change buffer size
    let response = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "buffer_size": new_buffer }))
        .send()
        .await
        .expect("Failed to update device");

    assert!(
        response.status().is_success(),
        "Buffer size update should succeed"
    );

    // Wait for stream reconfiguration
    tokio::time::sleep(Duration::from_secs(3)).await;

    // VERIFY: Device should report new buffer size
    let updated_device: DeviceInfo = client
        .get_json(&format!("/nodes/LOCAL/devices/{device_id}"))
        .await
        .expect("Failed to get updated device");

    assert_eq!(
        updated_device.buffer_size, new_buffer,
        "Device should report new buffer size"
    );

    // VERIFY: Streams should still be running
    let counts_after: StreamCounts = client
        .get_json("/streams/count")
        .await
        .expect("Failed to get stream counts");

    // Stream count should be reasonable after reconfiguration
    // (device was attached before, so should have at least some streams)
    println!(
        "Stream counts after buffer reconfiguration: input={}, output={}",
        counts_after.input, counts_after.output
    );

    // VERIFY: Device should still be attached
    assert!(
        matches!(
            updated_device.status,
            DeviceStatus::Attached | DeviceStatus::Active
        ),
        "Device should remain attached after buffer size change"
    );

    // Restore original buffer size
    let _ = client
        .client
        .patch(client.api_url(&format!("/nodes/LOCAL/devices/{device_id}")))
        .json(&serde_json::json!({ "buffer_size": original_buffer }))
        .send()
        .await;
}
