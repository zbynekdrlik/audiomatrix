//! API client for communicating with AudioMatrix service.
//!
//! Uses shared types from ram-api for type safety.

use gloo_net::http::Request;
use ram_api::models::{DeviceInfo, NodeInfo, RouteDefinition};

/// API error type.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub message: String,
    pub status: Option<u16>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(status) = self.status {
            write!(f, "API error ({}): {}", status, self.message)
        } else {
            write!(f, "API error: {}", self.message)
        }
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        Self {
            message: err.to_string(),
            status: None,
        }
    }
}

/// Result type for API operations.
pub type ApiResult<T> = Result<T, ApiError>;

/// Gets the base URL for API calls.
/// In browser, this is relative to the current origin.
fn api_base() -> String {
    // When running in browser, use relative URL
    "/api/v1".to_string()
}

/// Fetches all nodes.
pub async fn get_nodes() -> ApiResult<Vec<NodeInfo>> {
    let url = format!("{}/nodes", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.status_text(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Fetches a single node by ID.
pub async fn get_node(node_id: &str) -> ApiResult<NodeInfo> {
    let url = format!("{}/nodes/{}", api_base(), node_id);
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.status_text(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Fetches devices for a node.
pub async fn get_devices(node_id: &str) -> ApiResult<Vec<DeviceInfo>> {
    let url = format!("{}/nodes/{}/devices", api_base(), node_id);
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.status_text(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Fetches all routes.
pub async fn get_routes() -> ApiResult<Vec<RouteDefinition>> {
    let url = format!("{}/routes", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.status_text(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Route creation response.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RouteCreatedResponse {
    pub id: String,
}

/// Creates a new route.
pub async fn create_route(route: &RouteDefinition) -> ApiResult<RouteCreatedResponse> {
    let url = format!("{}/routes", api_base());
    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(route).map_err(|e| ApiError {
            message: e.to_string(),
            status: None,
        })?)?
        .send()
        .await?;

    if !response.ok() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(ApiError {
            message: error_text,
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Updates an existing route.
pub async fn update_route(route_id: &str, route: &RouteDefinition) -> ApiResult<()> {
    let url = format!("{}/routes/{}", api_base(), urlencoding::encode(route_id));
    let response = Request::put(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(route).map_err(|e| ApiError {
            message: e.to_string(),
            status: None,
        })?)?
        .send()
        .await?;

    if !response.ok() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(ApiError {
            message: error_text,
            status: Some(response.status()),
        });
    }

    Ok(())
}

/// Deletes a route.
pub async fn delete_route(route_id: &str) -> ApiResult<()> {
    let url = format!("{}/routes/{}", api_base(), urlencoding::encode(route_id));
    let response = Request::delete(&url).send().await?;

    if !response.ok() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(ApiError {
            message: error_text,
            status: Some(response.status()),
        });
    }

    Ok(())
}

/// Health check response.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub hostname: String,
}

/// Checks service health.
pub async fn health_check() -> ApiResult<HealthResponse> {
    let url = format!("{}/health", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.status_text(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_returns_relative_path() {
        assert_eq!(api_base(), "/api/v1");
    }

    #[test]
    fn api_error_display() {
        let err = ApiError {
            message: "Not found".to_string(),
            status: Some(404),
        };
        assert_eq!(format!("{}", err), "API error (404): Not found");
    }
}
