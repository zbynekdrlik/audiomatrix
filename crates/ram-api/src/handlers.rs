//! API request handlers.

use axum::extract::Path;
use axum::Json;

use crate::models::{DeviceInfo, HealthResponse, NodeInfo};
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

/// List all nodes.
///
/// # Errors
///
/// Returns an error if node listing fails.
pub async fn list_nodes() -> Result<Json<Vec<NodeInfo>>> {
    // TODO: Implement with actual node discovery
    Ok(Json(vec![]))
}

/// Get a specific node.
///
/// # Errors
///
/// Returns an error if the node is not found.
pub async fn get_node(Path(id): Path<String>) -> Result<Json<NodeInfo>> {
    // TODO: Implement
    Err(crate::Error::NotFound(format!("node: {id}")))
}

/// List devices for a node.
///
/// # Errors
///
/// Returns an error if device listing fails.
pub async fn list_devices(Path(_node_id): Path<String>) -> Result<Json<Vec<DeviceInfo>>> {
    // TODO: Implement
    Ok(Json(vec![]))
}

/// Get a specific device.
///
/// # Errors
///
/// Returns an error if the device is not found.
pub async fn get_device(
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<DeviceInfo>> {
    // TODO: Implement
    Err(crate::Error::NotFound(format!(
        "device: {node_id}/{device_id}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn list_nodes_returns_empty() {
        let result = list_nodes().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_node_returns_not_found() {
        let result = get_node(Path("test-node".into())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_devices_returns_empty() {
        let result = list_devices(Path("test-node".into())).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_device_returns_not_found() {
        let result = get_device(Path(("node".into(), "device".into()))).await;
        assert!(result.is_err());
    }
}
