//! API request handlers.

use axum::extract::{Path, State};
use axum::Json;

use crate::models::{DeviceInfo, HealthResponse, NodeInfo, RouteDefinition};
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
    let id = state.upsert_route(route);

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
    let new_id = state.upsert_route(route.clone());

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
    state
        .remove_route(&id)
        .ok_or_else(|| crate::Error::NotFound(format!("route: {id}")))?;

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
