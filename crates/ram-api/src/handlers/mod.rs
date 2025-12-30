//! API request handlers.

mod subscriptions;

use axum::extract::{Path, State};
use axum::Json;
use urlencoding;

use crate::models::{
    AttachDeviceRequest, AttachDeviceResponse, BulkChannelLabelsRequest, ChannelInfo,
    CreateVirtualDeviceRequest, DeviceGeneratorResponse, DeviceInfo, GeneratorStatus,
    HealthResponse, LatencyInfo, NodeInfo, RouteDefinition, SetAllGeneratorsRequest,
    SetGeneratorRequest, StreamInfo, SubscriptionInfo, SubscriptionStatsResponse,
    UpdateChannelLabelRequest, UpdateDeviceRequest, UpdateVirtualDeviceRequest,
    VirtualDeviceResponse,
};
use crate::state::AppState;
use crate::websocket::{RouteUpdate, WsEvent};
use crate::Result;

// Re-export subscription handler
pub use subscriptions::create_subscription;

/// Supported sample rates for virtual devices.
const SUPPORTED_SAMPLE_RATES: [u32; 3] = [44100, 48000, 96000];

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
pub async fn list_nodes(State(state): State<AppState>) -> Result<Json<Vec<NodeInfo>>> {
    Ok(Json(state.all_nodes()))
}

/// Get a specific node.
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
pub async fn list_devices(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<DeviceInfo>>> {
    if state.is_local_node(&node_id) {
        Ok(Json(state.all_devices()))
    } else {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/devices",
                    addr, node.api_port
                );
                match reqwest::get(&url).await {
                    Ok(resp) => match resp.json::<Vec<DeviceInfo>>().await {
                        Ok(devices) => return Ok(Json(devices)),
                        Err(e) => {
                            tracing::warn!("Failed to parse devices from {}: {}", node_id, e);
                        },
                    },
                    Err(e) => {
                        tracing::warn!("Failed to fetch devices from {}: {}", node_id, e);
                    },
                }
            }
        }
        Ok(Json(vec![]))
    }
}

/// Get a specific device.
pub async fn get_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<DeviceInfo>> {
    if state.is_local_node(&node_id) {
        state
            .get_device(&device_id)
            .map(Json)
            .ok_or_else(|| crate::Error::NotFound(format!("device: {device_id}")))
    } else {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/devices/{}",
                    addr,
                    node.api_port,
                    urlencoding::encode(&device_id)
                );
                if let Ok(resp) = reqwest::get(&url).await {
                    if let Ok(device) = resp.json::<DeviceInfo>().await {
                        return Ok(Json(device));
                    }
                }
            }
        }
        Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )))
    }
}

/// Attach a device for routing.
pub async fn attach_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
    Json(req): Json<AttachDeviceRequest>,
) -> Result<Json<AttachDeviceResponse>> {
    if !state.is_local_node(&node_id) {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/devices/{}/attach",
                    addr,
                    node.api_port,
                    urlencoding::encode(&device_id)
                );
                let client = reqwest::Client::new();
                match client.post(&url).json(&req).send().await {
                    Ok(resp) => match resp.json::<AttachDeviceResponse>().await {
                        Ok(result) => return Ok(Json(result)),
                        Err(e) => {
                            return Ok(Json(AttachDeviceResponse {
                                success: false,
                                device: None,
                                error: Some(format!("Failed to parse response: {e}")),
                            }));
                        },
                    },
                    Err(e) => {
                        return Ok(Json(AttachDeviceResponse {
                            success: false,
                            device: None,
                            error: Some(format!("Failed to connect to remote node: {e}")),
                        }));
                    },
                }
            }
        }
        return Ok(Json(AttachDeviceResponse {
            success: false,
            device: None,
            error: Some(format!("Remote node not found: {node_id}")),
        }));
    }

    match state.attach_device(&device_id, req.display_name.clone()) {
        Ok(device) => {
            let _ = state.broadcast_event(WsEvent::DeviceAttached {
                device_id: device_id.clone(),
                device_name: device.name.clone(),
                display_name: device.display_name.clone(),
            });
            Ok(Json(AttachDeviceResponse {
                success: true,
                device: Some(device),
                error: None,
            }))
        },
        Err(e) => Ok(Json(AttachDeviceResponse {
            success: false,
            device: None,
            error: Some(e),
        })),
    }
}

/// Detach a device from routing.
pub async fn detach_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<AttachDeviceResponse>> {
    if !state.is_local_node(&node_id) {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/devices/{}/detach",
                    addr,
                    node.api_port,
                    urlencoding::encode(&device_id)
                );
                let client = reqwest::Client::new();
                match client.post(&url).send().await {
                    Ok(resp) => match resp.json::<AttachDeviceResponse>().await {
                        Ok(result) => return Ok(Json(result)),
                        Err(e) => {
                            return Ok(Json(AttachDeviceResponse {
                                success: false,
                                device: None,
                                error: Some(format!("Failed to parse response: {e}")),
                            }));
                        },
                    },
                    Err(e) => {
                        return Ok(Json(AttachDeviceResponse {
                            success: false,
                            device: None,
                            error: Some(format!("Failed to connect to remote node: {e}")),
                        }));
                    },
                }
            }
        }
        return Ok(Json(AttachDeviceResponse {
            success: false,
            device: None,
            error: Some(format!("Remote node not found: {node_id}")),
        }));
    }

    match state.detach_device(&device_id) {
        Ok(device) => {
            let _ = state.broadcast_event(WsEvent::DeviceDetached {
                device_id: device_id.clone(),
            });
            Ok(Json(AttachDeviceResponse {
                success: true,
                device: Some(device),
                error: None,
            }))
        },
        Err(e) => Ok(Json(AttachDeviceResponse {
            success: false,
            device: None,
            error: Some(e),
        })),
    }
}

/// Update device settings.
pub async fn update_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
    Json(req): Json<UpdateDeviceRequest>,
) -> Result<Json<DeviceInfo>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )));
    }

    state
        .update_device(&device_id, req.display_name)
        .map(Json)
        .map_err(crate::Error::BadRequest)
}

/// Get channels for a device.
pub async fn get_device_channels(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<Vec<ChannelInfo>>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )));
    }

    state
        .get_device_channels(&device_id)
        .map(Json)
        .ok_or_else(|| crate::Error::NotFound(format!("device: {device_id}")))
}

/// Update a channel label.
pub async fn update_channel_label(
    State(state): State<AppState>,
    Path((node_id, device_id, channel)): Path<(String, String, u16)>,
    Json(req): Json<UpdateChannelLabelRequest>,
) -> Result<Json<ChannelInfo>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )));
    }

    state
        .set_channel_label(&device_id, channel, req.label)
        .map(Json)
        .map_err(crate::Error::BadRequest)
}

/// Update multiple channel labels at once.
pub async fn bulk_update_channel_labels(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
    Json(req): Json<BulkChannelLabelsRequest>,
) -> Result<Json<Vec<ChannelInfo>>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::NotFound(format!(
            "device: {node_id}/{device_id}"
        )));
    }

    state
        .set_channel_labels(&device_id, req.labels)
        .map(Json)
        .map_err(crate::Error::BadRequest)
}

// --- Virtual Device Handlers ---

/// List virtual devices.
pub async fn list_virtual_devices(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<DeviceInfo>>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::NotFound(format!("node: {node_id}")));
    }
    Ok(Json(state.list_virtual_devices()))
}

/// Create a virtual device.
pub async fn create_virtual_device(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Json(req): Json<CreateVirtualDeviceRequest>,
) -> Result<Json<VirtualDeviceResponse>> {
    if !state.is_local_node(&node_id) {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/virtual-devices",
                    addr, node.api_port
                );
                let client = reqwest::Client::new();
                match client.post(&url).json(&req).send().await {
                    Ok(resp) => match resp.json::<VirtualDeviceResponse>().await {
                        Ok(result) => return Ok(Json(result)),
                        Err(e) => {
                            return Ok(Json(VirtualDeviceResponse {
                                success: false,
                                device: None,
                                error: Some(format!("Failed to parse response: {e}")),
                            }));
                        },
                    },
                    Err(e) => {
                        return Ok(Json(VirtualDeviceResponse {
                            success: false,
                            device: None,
                            error: Some(format!("Failed to connect to remote node: {e}")),
                        }));
                    },
                }
            }
        }
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(format!("Remote node not found: {node_id}")),
        }));
    }

    if !SUPPORTED_SAMPLE_RATES.contains(&req.sample_rate) {
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(format!(
                "Unsupported sample rate: {}. Supported: {:?}",
                req.sample_rate, SUPPORTED_SAMPLE_RATES
            )),
        }));
    }

    if req.input_channels == 0 && req.output_channels == 0 {
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(
                "At least one of input_channels or output_channels must be > 0".to_string(),
            ),
        }));
    }

    if req.input_channels > 256 || req.output_channels > 256 {
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some("Channel count cannot exceed 256".to_string()),
        }));
    }

    match state.create_virtual_device(
        req.name,
        req.input_channels,
        req.output_channels,
        req.sample_rate,
        req.buffer_size,
    ) {
        Ok(device) => Ok(Json(VirtualDeviceResponse {
            success: true,
            device: Some(device),
            error: None,
        })),
        Err(e) => Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(e),
        })),
    }
}

/// Update a virtual device.
pub async fn update_virtual_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
    Json(req): Json<UpdateVirtualDeviceRequest>,
) -> Result<Json<VirtualDeviceResponse>> {
    if !state.is_local_node(&node_id) {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/virtual-devices/{}",
                    addr,
                    node.api_port,
                    urlencoding::encode(&device_id)
                );
                let client = reqwest::Client::new();
                match client.put(&url).json(&req).send().await {
                    Ok(resp) => match resp.json::<VirtualDeviceResponse>().await {
                        Ok(result) => return Ok(Json(result)),
                        Err(e) => {
                            return Ok(Json(VirtualDeviceResponse {
                                success: false,
                                device: None,
                                error: Some(format!("Failed to parse response: {e}")),
                            }));
                        },
                    },
                    Err(e) => {
                        return Ok(Json(VirtualDeviceResponse {
                            success: false,
                            device: None,
                            error: Some(format!("Failed to connect to remote node: {e}")),
                        }));
                    },
                }
            }
        }
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(format!("Remote node not found: {node_id}")),
        }));
    }

    if let Some(rate) = req.sample_rate {
        if !SUPPORTED_SAMPLE_RATES.contains(&rate) {
            return Ok(Json(VirtualDeviceResponse {
                success: false,
                device: None,
                error: Some(format!(
                    "Unsupported sample rate: {}. Supported: {:?}",
                    rate, SUPPORTED_SAMPLE_RATES
                )),
            }));
        }
    }

    match state.update_virtual_device(
        &device_id,
        req.name,
        req.input_channels,
        req.output_channels,
        req.sample_rate,
        req.buffer_size,
    ) {
        Ok(device) => Ok(Json(VirtualDeviceResponse {
            success: true,
            device: Some(device),
            error: None,
        })),
        Err(e) => Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(e),
        })),
    }
}

/// Delete a virtual device.
pub async fn delete_virtual_device(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<VirtualDeviceResponse>> {
    if !state.is_local_node(&node_id) {
        // Proxy to remote node
        if let Some(node) = state.get_remote_node(&node_id) {
            if let Some(addr) = node.addresses.first() {
                let url = format!(
                    "http://{}:{}/api/v1/nodes/local/virtual-devices/{}",
                    addr,
                    node.api_port,
                    urlencoding::encode(&device_id)
                );
                let client = reqwest::Client::new();
                match client.delete(&url).send().await {
                    Ok(resp) => match resp.json::<VirtualDeviceResponse>().await {
                        Ok(result) => return Ok(Json(result)),
                        Err(e) => {
                            return Ok(Json(VirtualDeviceResponse {
                                success: false,
                                device: None,
                                error: Some(format!("Failed to parse response: {e}")),
                            }));
                        },
                    },
                    Err(e) => {
                        return Ok(Json(VirtualDeviceResponse {
                            success: false,
                            device: None,
                            error: Some(format!("Failed to connect to remote node: {e}")),
                        }));
                    },
                }
            }
        }
        return Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(format!("Remote node not found: {node_id}")),
        }));
    }

    match state.delete_virtual_device(&device_id) {
        Ok(device) => Ok(Json(VirtualDeviceResponse {
            success: true,
            device: Some(device),
            error: None,
        })),
        Err(e) => Ok(Json(VirtualDeviceResponse {
            success: false,
            device: None,
            error: Some(e),
        })),
    }
}

// --- Wave Generator Handlers ---

/// Get generator status for all channels of a device.
pub async fn get_device_generator(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
) -> Result<Json<DeviceGeneratorResponse>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::BadRequest(format!(
            "Generator control only available on local node, not: {node_id}"
        )));
    }

    let channels = state.get_generator_status(&device_id);
    Ok(Json(DeviceGeneratorResponse {
        device_id,
        channels,
    }))
}

/// Set generator for a specific channel.
pub async fn set_channel_generator(
    State(state): State<AppState>,
    Path((node_id, device_id, channel)): Path<(String, String, u16)>,
    Json(req): Json<SetGeneratorRequest>,
) -> Result<Json<GeneratorStatus>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::BadRequest(format!(
            "Generator control only available on local node, not: {node_id}"
        )));
    }

    state.set_channel_generator(
        &device_id,
        channel,
        req.enabled,
        req.waveform,
        req.frequency,
        req.level_db,
    );

    Ok(Json(GeneratorStatus {
        channel,
        enabled: req.enabled,
        waveform: req.waveform,
        frequency: req.frequency,
        level_db: req.level_db,
    }))
}

/// Set generator for all channels of a device.
pub async fn set_all_generators(
    State(state): State<AppState>,
    Path((node_id, device_id)): Path<(String, String)>,
    Json(req): Json<SetAllGeneratorsRequest>,
) -> Result<Json<DeviceGeneratorResponse>> {
    if !state.is_local_node(&node_id) {
        return Err(crate::Error::BadRequest(format!(
            "Generator control only available on local node, not: {node_id}"
        )));
    }

    state.set_all_generators(
        &device_id,
        req.enabled,
        req.waveform,
        req.frequency,
        req.level_db,
    );

    let channels = state.get_generator_status(&device_id);
    Ok(Json(DeviceGeneratorResponse {
        device_id,
        channels,
    }))
}

// --- Route Handlers ---

/// List all routes.
pub async fn list_routes(State(state): State<AppState>) -> Result<Json<Vec<RouteDefinition>>> {
    Ok(Json(state.all_routes()))
}

/// Get a specific route.
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
pub async fn create_route(
    State(state): State<AppState>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteCreatedResponse>> {
    let id = state
        .upsert_route(route)
        .map_err(crate::Error::BadRequest)?;

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "added".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteCreatedResponse { id }))
}

/// Update an existing route.
pub async fn update_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(route): Json<RouteDefinition>,
) -> Result<Json<RouteDefinition>> {
    if state.get_route(&id).is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    let new_id = state
        .upsert_route(route.clone())
        .map_err(crate::Error::BadRequest)?;

    if new_id != id {
        let _ = state.remove_route(&id);
    }

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "modified".into(),
        route_id: new_id,
    }));

    Ok(Json(route))
}

/// Delete a route.
pub async fn delete_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RouteDeletedResponse>> {
    let removed = state.remove_route(&id).map_err(crate::Error::Internal)?;

    if removed.is_none() {
        return Err(crate::Error::NotFound(format!("route: {id}")));
    }

    state.broadcast_event(WsEvent::RouteChanged(RouteUpdate {
        action: "removed".into(),
        route_id: id.clone(),
    }));

    Ok(Json(RouteDeletedResponse { id }))
}

/// Response for route creation.
#[derive(Debug, serde::Serialize)]
pub struct RouteCreatedResponse {
    pub id: String,
}

/// Response for route deletion.
#[derive(Debug, serde::Serialize)]
pub struct RouteDeletedResponse {
    pub id: String,
}

// --- Stream Handlers ---

/// List all active streams.
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
    pub input_streams: usize,
    pub output_streams: usize,
}

// --- Subscription Handlers ---

/// List all subscriptions.
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

// --- Latency Handlers ---

/// Get latency information for a route.
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
    use crate::models::{DeviceStatus, DeviceType};

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
            display_name: None,
            device_type: DeviceType::Input,
            input_channels: 2,
            output_channels: 0,
            sample_rate: 48000,
            buffer_size: 256,
            is_virtual: false,
            status: DeviceStatus::Available,
            backend: None,
        });

        let result = list_devices(State(state), Path("local".into())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
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
}
