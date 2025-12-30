//! API client for communicating with AudioMatrix service.
//!
//! Uses shared types from ram-api for type safety.

use gloo_net::http::Request;
use ram_api::models::{
    AttachDeviceResponse, ChannelInfo, CreateVirtualDeviceRequest, DeviceGeneratorResponse,
    DeviceInfo, NodeInfo, RouteDefinition, SetGeneratorRequest, StreamInfo, SubscriptionInfo,
    SubscriptionStatsResponse, VirtualDeviceResponse,
};

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

// ============================================================================
// Device Management APIs
// ============================================================================

/// Attaches a device to AudioMatrix for routing.
pub async fn attach_device(
    node_id: &str,
    device_id: &str,
    display_name: Option<&str>,
) -> ApiResult<AttachDeviceResponse> {
    let url = format!(
        "{}/nodes/{}/devices/{}/attach",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let body = serde_json::json!({ "display_name": display_name });

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Detaches a device from AudioMatrix.
pub async fn detach_device(node_id: &str, device_id: &str) -> ApiResult<()> {
    let url = format!(
        "{}/nodes/{}/devices/{}/detach",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let response = Request::post(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    Ok(())
}

/// Updates device properties (display name, sample rate, buffer size).
pub async fn update_device(
    node_id: &str,
    device_id: &str,
    display_name: Option<&str>,
    sample_rate: Option<u32>,
    buffer_size: Option<u32>,
) -> ApiResult<DeviceInfo> {
    let url = format!(
        "{}/nodes/{}/devices/{}",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let body = serde_json::json!({
        "display_name": display_name,
        "sample_rate": sample_rate,
        "buffer_size": buffer_size
    });

    let response = Request::patch(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

// ============================================================================
// Channel Label APIs
// ============================================================================

/// Gets channel information for a device.
pub async fn get_device_channels(node_id: &str, device_id: &str) -> ApiResult<Vec<ChannelInfo>> {
    let url = format!(
        "{}/nodes/{}/devices/{}/channels",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Updates a single channel label.
pub async fn update_channel_label(
    node_id: &str,
    device_id: &str,
    channel: u16,
    label: &str,
) -> ApiResult<ChannelInfo> {
    let url = format!(
        "{}/nodes/{}/devices/{}/channels/{}",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id),
        channel
    );

    let body = serde_json::json!({ "label": label });

    let response = Request::patch(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

// ============================================================================
// Virtual Device APIs
// ============================================================================

/// Lists virtual devices on a node.
pub async fn list_virtual_devices(node_id: &str) -> ApiResult<Vec<DeviceInfo>> {
    let url = format!(
        "{}/nodes/{}/virtual-devices",
        api_base(),
        urlencoding::encode(node_id)
    );

    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Creates a new virtual ASIO device.
pub async fn create_virtual_device(
    node_id: &str,
    request: &CreateVirtualDeviceRequest,
) -> ApiResult<VirtualDeviceResponse> {
    let url = format!(
        "{}/nodes/{}/virtual-devices",
        api_base(),
        urlencoding::encode(node_id)
    );

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(request).map_err(|e| ApiError {
            message: e.to_string(),
            status: None,
        })?)?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Deletes a virtual device.
pub async fn delete_virtual_device(node_id: &str, device_id: &str) -> ApiResult<()> {
    let url = format!(
        "{}/nodes/{}/virtual-devices/{}",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let response = Request::delete(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    Ok(())
}

// ============================================================================
// Generator APIs
// ============================================================================

/// Gets generator status for all channels of a device.
pub async fn get_device_generator(
    node_id: &str,
    device_id: &str,
) -> ApiResult<DeviceGeneratorResponse> {
    let url = format!(
        "{}/nodes/{}/devices/{}/generator",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Sets generator for a specific channel.
pub async fn set_channel_generator(
    node_id: &str,
    device_id: &str,
    channel: u16,
    request: &SetGeneratorRequest,
) -> ApiResult<()> {
    let url = format!(
        "{}/nodes/{}/devices/{}/channels/{}/generator",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id),
        channel
    );

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(request).map_err(|e| ApiError {
            message: e.to_string(),
            status: None,
        })?)?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    Ok(())
}

/// Sets generator for all channels of a device.
pub async fn set_all_generators(
    node_id: &str,
    device_id: &str,
    request: &SetGeneratorRequest,
) -> ApiResult<()> {
    let url = format!(
        "{}/nodes/{}/devices/{}/generator",
        api_base(),
        urlencoding::encode(node_id),
        urlencoding::encode(device_id)
    );

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(request).map_err(|e| ApiError {
            message: e.to_string(),
            status: None,
        })?)?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    Ok(())
}

// ============================================================================
// Stream and Subscription APIs
// ============================================================================

/// Lists active audio streams.
pub async fn list_streams() -> ApiResult<Vec<StreamInfo>> {
    let url = format!("{}/streams", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Lists active subscriptions.
pub async fn list_subscriptions() -> ApiResult<Vec<SubscriptionInfo>> {
    let url = format!("{}/subscriptions", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Gets subscription statistics.
pub async fn get_subscription_stats() -> ApiResult<SubscriptionStatsResponse> {
    let url = format!("{}/subscriptions/stats", api_base());
    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

// ============================================================================
// Latency API
// ============================================================================

/// Latency breakdown for a route.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LatencyInfo {
    pub route_id: String,
    pub input_buffer_ms: f32,
    pub ring_buffer_ms: f32,
    pub output_buffer_ms: f32,
    pub network_ms: f32,
    pub processing_ms: f32,
    pub total_ms: f32,
    pub is_local: bool,
}

/// Gets latency information for a route.
pub async fn get_route_latency(route_id: &str) -> ApiResult<LatencyInfo> {
    let url = format!(
        "{}/routes/{}/latency",
        api_base(),
        urlencoding::encode(route_id)
    );

    let response = Request::get(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
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
