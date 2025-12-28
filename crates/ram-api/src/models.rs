//! API data models.

use serde::{Deserialize, Serialize};

/// Health check response.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status.
    pub status: String,
    /// Service version.
    pub version: String,
    /// Hostname.
    pub hostname: String,
}

/// Node information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node identifier.
    pub id: String,
    /// Node name.
    pub name: String,
    /// IP addresses.
    pub addresses: Vec<String>,
    /// API port.
    pub api_port: u16,
    /// VBAN port.
    pub vban_port: u16,
    /// Online status.
    pub online: bool,
}

/// Device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device identifier.
    pub id: String,
    /// Device name.
    pub name: String,
    /// Device type (input/output).
    pub device_type: DeviceType,
    /// Number of channels.
    pub channels: u8,
    /// Sample rate.
    pub sample_rate: u32,
    /// Whether this is a virtual device.
    pub is_virtual: bool,
}

/// Device type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    /// Input device (capture).
    Input,
    /// Output device (playback).
    Output,
}

/// Route definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDefinition {
    /// Source node.
    pub source_node: String,
    /// Source device.
    pub source_device: String,
    /// Source channel (1-based).
    pub source_channel: u16,
    /// Destination node.
    pub destination_node: String,
    /// Destination device.
    pub destination_device: String,
    /// Destination channel (1-based).
    pub destination_channel: u16,
    /// Volume (0.0 to 1.0+).
    pub volume: f32,
    /// Mute state.
    pub muted: bool,
}

/// Subscription request.
#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct SubscriptionRequest {
    /// Source node.
    pub source_node: String,
    /// Source device.
    pub source_device: String,
    /// Source channel (1-based).
    pub source_channel: u16,
}

/// Metering data.
#[derive(Debug, Serialize, Deserialize)]
pub struct MeteringData {
    /// Node identifier.
    pub node: String,
    /// Device identifier.
    pub device: String,
    /// Channel levels (dBFS).
    pub levels: Vec<f32>,
    /// Peak hold levels (dBFS).
    pub peaks: Vec<f32>,
}

/// Active stream information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// Device identifier.
    pub device_id: String,
    /// Device name.
    pub device_name: String,
    /// Stream direction.
    pub direction: StreamDirection,
    /// Number of channels.
    pub channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Whether the stream is running.
    pub running: bool,
}

/// Stream direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StreamDirection {
    /// Input stream (capture).
    Input,
    /// Output stream (playback).
    Output,
}

/// Subscription information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    /// Subscription ID.
    pub id: u64,
    /// Source node.
    pub source_node: String,
    /// Source device.
    pub source_device: String,
    /// Source channels.
    pub source_channels: Vec<u16>,
    /// Destination device.
    pub dest_device: String,
    /// Destination channels.
    pub dest_channels: Vec<u16>,
    /// VBAN stream name.
    pub vban_stream_name: String,
    /// Subscription state.
    pub state: SubscriptionState,
    /// Sample rate.
    pub sample_rate: u32,
}

/// Subscription state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionState {
    /// Subscription pending.
    Pending,
    /// Subscription active.
    Active,
    /// Subscription failed.
    Failed,
}

/// Subscription statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionStatsResponse {
    /// Total outgoing subscriptions.
    pub outgoing_total: usize,
    /// Active outgoing subscriptions.
    pub outgoing_active: usize,
    /// Pending outgoing subscriptions.
    pub outgoing_pending: usize,
    /// Total incoming subscriptions.
    pub incoming_total: usize,
    /// Active incoming subscriptions.
    pub incoming_active: usize,
}

/// Latency report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyInfo {
    /// Route identifier.
    pub route_id: String,
    /// Input buffer latency (ms).
    pub input_buffer_ms: f32,
    /// Ring buffer latency (ms).
    pub ring_buffer_ms: f32,
    /// Output buffer latency (ms).
    pub output_buffer_ms: f32,
    /// Network latency (ms).
    pub network_ms: f32,
    /// Processing overhead (ms).
    pub processing_ms: f32,
    /// Total latency (ms).
    pub total_ms: f32,
    /// Whether this is a local route.
    pub is_local: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_serialization() {
        let response = HealthResponse {
            status: "ok".into(),
            version: "1.0.0".into(),
            hostname: "test-host".into(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"version\":\"1.0.0\""));

        let parsed: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, "ok");
        assert_eq!(parsed.version, "1.0.0");
        assert_eq!(parsed.hostname, "test-host");
    }

    #[test]
    fn node_info_serialization() {
        let node = NodeInfo {
            id: "node-1".into(),
            name: "Test Node".into(),
            addresses: vec!["192.168.1.1".into()],
            api_port: 8080,
            vban_port: 6980,
            online: true,
        };

        let json = serde_json::to_string(&node).unwrap();
        let parsed: NodeInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "node-1");
        assert_eq!(parsed.name, "Test Node");
        assert_eq!(parsed.addresses, vec!["192.168.1.1"]);
        assert_eq!(parsed.api_port, 8080);
        assert_eq!(parsed.vban_port, 6980);
        assert!(parsed.online);
    }

    #[test]
    fn device_info_serialization() {
        let device = DeviceInfo {
            id: "device-1".into(),
            name: "Test Device".into(),
            device_type: DeviceType::Input,
            channels: 2,
            sample_rate: 48000,
            is_virtual: false,
        };

        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("\"device_type\":\"input\""));

        let parsed: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "device-1");
        assert_eq!(parsed.device_type, DeviceType::Input);
    }

    #[test]
    fn device_type_output() {
        let device = DeviceInfo {
            id: "device-2".into(),
            name: "Output Device".into(),
            device_type: DeviceType::Output,
            channels: 8,
            sample_rate: 96000,
            is_virtual: true,
        };

        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("\"device_type\":\"output\""));

        let parsed: DeviceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.device_type, DeviceType::Output);
        assert!(parsed.is_virtual);
    }

    #[test]
    fn route_definition_serialization() {
        let route = RouteDefinition {
            source_node: "node-a".into(),
            source_device: "device-1".into(),
            source_channel: 1,
            destination_node: "node-b".into(),
            destination_device: "device-2".into(),
            destination_channel: 2,
            volume: 0.8,
            muted: false,
        };

        let json = serde_json::to_string(&route).unwrap();
        let parsed: RouteDefinition = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source_node, "node-a");
        assert_eq!(parsed.destination_channel, 2);
        assert!((parsed.volume - 0.8).abs() < 0.001);
        assert!(!parsed.muted);
    }

    #[test]
    fn subscription_request_serialization() {
        let sub = SubscriptionRequest {
            source_node: "remote-node".into(),
            source_device: "input-device".into(),
            source_channel: 3,
        };

        let json = serde_json::to_string(&sub).unwrap();
        let parsed: SubscriptionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.source_node, "remote-node");
        assert_eq!(parsed.source_device, "input-device");
        assert_eq!(parsed.source_channel, 3);
    }

    #[test]
    fn metering_data_serialization() {
        let metering = MeteringData {
            node: "node-1".into(),
            device: "device-1".into(),
            levels: vec![-12.0, -18.0],
            peaks: vec![-6.0, -9.0],
        };

        let json = serde_json::to_string(&metering).unwrap();
        let parsed: MeteringData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.levels.len(), 2);
        assert_eq!(parsed.peaks.len(), 2);
        assert!((parsed.levels[0] - (-12.0)).abs() < 0.001);
    }
}
