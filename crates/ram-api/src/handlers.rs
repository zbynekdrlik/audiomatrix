//! API request handlers.

use axum::extract::{Path, State};
use axum::Json;

use crate::models::{
    DeviceInfo, HealthResponse, LatencyInfo, NodeInfo, RouteDefinition, StreamInfo,
    SubscriptionInfo, SubscriptionRequest, SubscriptionResponse, SubscriptionStatsResponse,
};
use crate::state::AppState;
use crate::websocket::{RouteUpdate, WsEvent};
use crate::Result;

/// Health check handler.
pub async fn health() -> Json<HealthResponse> {
    let hostname =
        hostname::get().map_or_else(|_| "unknown".into(), |h| h.to_string_lossy().into_owned());

    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        hostname,
    })
}

// --- Node Handlers ---

/// List all nodes.
///
/// # Errors
///
/// Returns an error if node listing fails.
pub async fn list_nodes(State(state): State<AppState>) -> Result<Json<Vec<NodeInfo>>> {
    Ok(Json(state.all_nodes()))
}

/// Get a specific node.
///
/// # Errors
///
/// Returns an error if the node is not found.
pub async fn get_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeInfo>> {
    state
        .get_node(&id)
        .map(Json)
        .ok_or_else(|| crate::Error::NotFound(format!("node: {id}")))
}

// --- Device Handlers ---

/// List devices for a node.
///
/// # Errors
///
/// Returns an error if device listing fails.
pub async fn list_devices(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<DeviceInfo>>> {
    let local_id = state.local_node().id;
    if node_id == local_id || node_id == "local" {
        Ok(Json(state.all_devices()))
    } else {
        // Remote node - currently not implemented
        Ok(Json(vec![]))
    }
}

/// Get a specific device.
///
/// # Errors
///
/// Returns an error if the device is not found.
pub async fn get_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<DeviceInfo>> {
    let local_id = state.local_node().id;
    if node_id == local_id || node_id == "local" {
        state
            .get_device(&device_id)
            .map(Json)
            .ok_or_else(|| crate::Error::NotFound(format!("device: {device_id}")))
    } else {
        Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )))
    }
}

// --- Route Handlers ---

/// List all routes.
///
/// # Errors
///
/// Returns an error if route listing fails.
pub async fn list_routes(State(state): State<AppState>) -> Result<Json<Vec<RouteDefinition>>> {
    Ok(Json(state.all_routes()))
}

/// Get a specific route.
///
/// # Errors
///
/// Returns an error if the route is not found.
pub async fn get_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteDefinition>> {
    state
        .get_route(&id)
        .map(Json)
        .ok_or_else(|| crate::Error::NotFound(format!("route: {id}")))
}

/// Create a new route.
///
/// # Errors
///
/// Returns an error if route creation fails.
pub async fn create_route(
    State(state): State<AppState>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteCreatedResponse>> {
    let id = state
        .upsert_route(route)
        .map_err(|e| crate::Error::BadRequest(e))?;

    // Broadcast route change event
    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "added".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteCreatedResponse { id }))
}

/// Update an existing route.
///
/// # Errors
///
/// Returns an error if the route is not found.
pub async fn update_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteDefinition>> {
    // Check if route exists
    if state.get_route(&id).is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    // Update route (may generate new ID if endpoints changed)
    let new_id = state
        .upsert_route(route.clone())
        .map_err(|e| crate::Error::BadRequest(e))?;

    // If ID changed, remove old route
    if new_id != id {
        let _ = state.remove_route(&id);
    }

    // Broadcast route change event
    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "modified".into(),
        route_id: new_id,
    }));

    Ok(Json(route))
}

/// Delete a route.
///
/// # Errors
///
/// Returns an error if the route is not found.
pub async fn delete_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteDeletedResponse>> {
    let removed = state
        .remove_route(&id)
        .map_err(|e| crate::Error::Internal(e))?;

    if removed.is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    // Broadcast route change event
    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "removed".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteDeletedResponse { id }))
}

/// Response for route creation.
#[derive(Debug, serde::Serialize)]
pub struct RouteCreatedResponse {
    /// Created route ID.
    pub id: String,
}

/// Response for route deletion.
#[derive(Debug, serde::Serialize)]
pub struct RouteDeletedResponse {
    /// Deleted route ID.
    pub id: String,
}

// --- Stream Handlers ---

/// List all active streams.
///
/// # Errors
///
/// Returns an error if stream listing fails.
pub async fn list_streams(State(state): State<AppState>) -> Result<Json<Vec<StreamInfo>>> {
    Ok(Json(state.all_streams()))
}

/// Get stream count.
pub async fn get_stream_count(State(state): State<AppState>) -> Json<StreamCountResponse> {
    let (input, output) = state.stream_counts();
    Json(StreamCountResponse {
        input_streams: input,
        output_streams: output,
    })
}

/// Response for stream count.
#[derive(Debug, serde::Serialize)]
pub struct StreamCountResponse {
    /// Number of active input streams.
    pub input_streams: usize,
    /// Number of active output streams.
    pub output_streams: usize,
}

// --- Subscription Handlers ---

/// List all subscriptions.
///
/// # Errors
///
/// Returns an error if subscription listing fails.
pub async fn list_subscriptions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SubscriptionInfo>>> {
    Ok(Json(state.all_subscriptions()))
}

/// Get subscription statistics.
pub async fn get_subscription_stats(
    State(state): State<AppState>,
) -> Json<SubscriptionStatsResponse> {
    Json(state.subscription_stats())
}

/// Create a subscription (called by destination node on source node).
///
/// This endpoint is called when a destination node wants to subscribe
/// to audio from this node. We validate the device exists and start
/// sending VBAN audio to the destination.
///
/// # Errors
///
/// Returns an error if the device doesn't exist or subscription fails.
pub async fn create_subscription(
    State(state): State<AppState>,
    Json(request): Json<SubscriptionRequest>,
) -> Result<Json<SubscriptionResponse>> {
    // Validate that the source device exists
    let device = state.get_device(&request.source_device);
    if device.is_none() {
        return Ok(Json(SubscriptionResponse {
            success: false,
            subscription_id: None,
            vban_stream_name: None,
            sample_rate: None,
            error: Some(format!("Device not found: {}", request.source_device)),
        }));
    }

    // If we have a route controller, create the subscription
    if let Some(controller) = state.route_controller() {
        let manager = controller.subscription_manager();

        // Convert the request to a core subscription request
        let dest_addr = request
            .destination_addr
            .parse()
            .map_err(|e| crate::Error::BadRequest(format!("Invalid address: {e}")))?;

        let core_request = ram_core::subscription::SubscribeRequest {
            request_id: 0, // Will be assigned by manager
            stream_name: request.stream_name.clone(),
            source_device: request.source_device.clone(),
            source_channels: request.source_channels.clone(),
            destination_node: request.destination_node.clone(),
            destination_addr: dest_addr,
            sample_rate: request.sample_rate,
        };

        // Handle the subscription request
        let ack = manager.handle_subscribe_request(core_request, |_device, _channels| {
            // For now, accept all devices that exist
            // In the future, validate channels
            true
        });

        if ack.result.is_success() {
            tracing::info!(
                "Subscription created: stream='{}' -> {}",
                ack.vban_stream_name,
                request.destination_addr
            );

            // Start VBAN sender for this subscription
            // First, ensure the input stream is running and get buffer indices
            tracing::info!(
                "Ensuring input stream for device '{}' channels {:?}",
                request.source_device,
                request.source_channels
            );
            let source_buffers = match controller
                .ensure_input_stream(&request.source_device, &request.source_channels)
            {
                Ok(buffers) => {
                    tracing::info!("Got buffer indices: {:?}", buffers);
                    buffers
                },
                Err(e) => {
                    tracing::error!("Failed to start input stream: {}", e);
                    return Ok(Json(SubscriptionResponse {
                        success: false,
                        subscription_id: None,
                        vban_stream_name: None,
                        sample_rate: None,
                        error: Some(format!("Failed to start input stream: {e}")),
                    }));
                },
            };

            // Start the VBAN sender
            tracing::info!(
                "Starting VBAN sender: sub_id={}, stream='{}', dest={}, buffers={:?}, channels={}",
                ack.subscription_id,
                ack.vban_stream_name,
                dest_addr,
                source_buffers,
                request.source_channels.len()
            );
            if let Err(e) = controller.start_vban_sender(
                ack.subscription_id,
                ack.vban_stream_name.clone(),
                dest_addr,
                source_buffers,
                request.source_channels.len() as u8,
            ) {
                tracing::error!("Failed to start VBAN sender: {}", e);
                return Ok(Json(SubscriptionResponse {
                    success: false,
                    subscription_id: None,
                    vban_stream_name: None,
                    sample_rate: None,
                    error: Some(format!("Failed to start VBAN sender: {e}")),
                }));
            }

            // Activate the subscription now that the VBAN sender is running
            manager.activate_incoming(ack.subscription_id);

            tracing::info!(
                "VBAN sender started for subscription {}: stream='{}'",
                ack.subscription_id,
                ack.vban_stream_name
            );

            Ok(Json(SubscriptionResponse {
                success: true,
                subscription_id: Some(ack.subscription_id),
                vban_stream_name: Some(ack.vban_stream_name),
                sample_rate: Some(ack.sample_rate),
                error: None,
            }))
        } else {
            Ok(Json(SubscriptionResponse {
                success: false,
                subscription_id: None,
                vban_stream_name: None,
                sample_rate: None,
                error: Some(format!("Subscription failed: {:?}", ack.result)),
            }))
        }
    } else {
        Ok(Json(SubscriptionResponse {
            success: false,
            subscription_id: None,
            vban_stream_name: None,
            sample_rate: None,
            error: Some("No route controller available".to_string()),
        }))
    }
}

// --- Latency Handlers ---

/// Get latency information for a route.
///
/// # Errors
///
/// Returns an error if the route is not found.
pub async fn get_route_latency(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LatencyInfo>> {
    state
        .get_route_latency(&id)
        .map(Json)
        .ok_or_else(|| crate::Error::NotFound(format!("route: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DeviceType;

    #[tokio::test]
    async fn health_returns_ok() {
        let response = health().await;
        assert_eq!(response.status, "ok");
    }

    #[tokio::test]
    async fn health_contains_version() {
        let response = health().await;
        assert!(!response.version.is_empty());
        assert_eq!(response.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn health_contains_hostname() {
        let response = health().await;
        assert!(!response.hostname.is_empty());
    }

    #[tokio::test]
    async fn list_nodes_returns_local() {
        let state = AppState::new("TestNode", 8080, 6980);
        let result = list_nodes(State(state)).await;
        assert!(result.is_ok());
        let nodes = result.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TestNode");
    }

    #[tokio::test]
    async fn get_node_finds_local() {
        let state = AppState::new("TestNode", 8080, 6980);
        let local_id = state.local_node().id.clone();
        let result = get_node(State(state), Path(local_id)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "TestNode");
    }

    #[tokio::test]
    async fn get_node_returns_not_found() {
        let state = AppState::default();
        let result = get_node(State(state), Path("nonexistent".into())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_devices_returns_empty() {
        let state = AppState::default();
        let result = list_devices(State(state), Path("local".into())).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_devices_with_registered_device() {
        let state = AppState::default();
        state.register_device(DeviceInfo {
            id: "device-1".into(),
            name: "Test Device".into(),
            device_type: DeviceType::Input,
            channels: 2,
            sample_rate: 48000,
            is_virtual: false,
        });

        let result = list_devices(State(state), Path("local".into())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_device_returns_not_found() {
        let state = AppState::default();
        let result = get_device(State(state), Path(("local".into(), "device".into()))).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_routes_returns_empty() {
        let state = AppState::default();
        let result = list_routes(State(state)).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_and_list_route() {
        let state = AppState::default();
        let route = RouteDefinition {
            source_node: "node-a".into(),
            source_device: "dev-1".into(),
            source_channel: 1,
            destination_node: "node-b".into(),
            destination_device: "dev-2".into(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        };

        let create_result = create_route(State(state.clone()), Json(route)).await;
        assert!(create_result.is_ok());

        let list_result = list_routes(State(state)).await;
        assert!(list_result.is_ok());
        assert_eq!(list_result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_route_returns_not_found() {
        let state = AppState::default();
        let result = get_route(State(state), Path("nonexistent".into())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_route_removes_it() {
        let state = AppState::default();
        let route = RouteDefinition {
            source_node: "node-a".into(),
            source_device: "dev-1".into(),
            source_channel: 1,
            destination_node: "node-b".into(),
            destination_device: "dev-2".into(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        };

        let create_result = create_route(State(state.clone()), Json(route)).await;
        let route_id = create_result.unwrap().id.clone();

        let delete_result = delete_route(State(state.clone()), Path(route_id)).await;
        assert!(delete_result.is_ok());

        let list_result = list_routes(State(state)).await;
        assert!(list_result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_nonexistent_route_returns_not_found() {
        let state = AppState::default();
        let result = delete_route(State(state), Path("nonexistent".into())).await;
        assert!(result.is_err());
    }
}
