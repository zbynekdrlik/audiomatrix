//! Routes endpoint E2E tests.

use ram_api::models::RouteDefinition;
use serde::Deserialize;

use crate::e2e::TestClient;

/// Route creation response.
#[derive(Debug, Deserialize)]
pub struct RouteResponse {
    pub id: String,
}

/// Test route listing.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_list_routes() {
    let client = TestClient::new();

    let response = client.get("/routes").await.expect("Failed to list routes");

    assert!(
        response.status().is_success(),
        "List routes failed with status: {}",
        response.status()
    );

    let routes: Vec<RouteDefinition> = response.json().await.expect("Failed to parse routes");

    // Routes list should be valid (may be empty)
    // Just verify we got a valid list back
    let _ = routes.len();
}

/// Test route creation with valid devices.
#[tokio::test]
#[ignore = "Requires running server with devices"]
async fn test_create_route() {
    let client = TestClient::new();

    // First, get available devices
    let devices_response = client
        .get("/nodes/LOCAL/devices")
        .await
        .expect("Failed to get devices");

    if !devices_response.status().is_success() {
        // Skip test if no devices available
        return;
    }

    let devices: Vec<ram_api::models::DeviceInfo> = devices_response
        .json()
        .await
        .expect("Failed to parse devices");

    // Find an input and output device
    let input_device = devices
        .iter()
        .find(|d| matches!(d.device_type, ram_api::models::DeviceType::Input));
    let output_device = devices
        .iter()
        .find(|d| matches!(d.device_type, ram_api::models::DeviceType::Output));

    if input_device.is_none() || output_device.is_none() {
        // Skip test if no suitable devices
        return;
    }

    let input_device = input_device.unwrap();
    let output_device = output_device.unwrap();

    let route = RouteDefinition {
        source_node: "LOCAL".to_string(),
        source_device: input_device.id.clone(),
        source_channel: 1,
        destination_node: "LOCAL".to_string(),
        destination_device: output_device.id.clone(),
        destination_channel: 1,
        volume: 1.0,
        muted: false,
    };

    let response = client
        .post_json("/routes", &route)
        .await
        .expect("Failed to create route");

    assert!(
        response.status().is_success(),
        "Create route failed with status: {}",
        response.status()
    );

    let route_response: RouteResponse = response.json().await.expect("Failed to parse response");
    assert!(!route_response.id.is_empty(), "Should return route ID");

    // Clean up: delete the created route
    let route_id = &route_response.id;
    let delete_response = client
        .delete(&format!("/routes/{route_id}"))
        .await
        .expect("Failed to delete route");

    assert!(
        delete_response.status().is_success(),
        "Delete route failed with status: {}",
        delete_response.status()
    );
}

/// Test route deletion returns 404 for nonexistent route.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_delete_nonexistent_route_returns_404() {
    let client = TestClient::new();

    let response = client
        .delete("/routes/nonexistent-route-id-12345")
        .await
        .expect("Failed to delete route");

    assert_eq!(
        response.status().as_u16(),
        404,
        "Nonexistent route should return 404"
    );
}
