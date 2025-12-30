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

/// Device information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device identifier (system-assigned).
    pub id: String,
    /// Device name (from driver/OS).
    pub name: String,
    /// User-defined display name (alias).
    /// If None, UI should display `name` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Device type (input/output/duplex).
    pub device_type: DeviceType,
    /// Number of input channels (0 for output-only devices).
    #[serde(default)]
    pub input_channels: u16,
    /// Number of output channels (0 for input-only devices).
    #[serde(default)]
    pub output_channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Buffer size in samples.
    #[serde(default)]
    pub buffer_size: u32,
    /// Whether this is a virtual device created by AudioMatrix.
    pub is_virtual: bool,
    /// Device attachment status (zero auto-connect policy).
    pub status: DeviceStatus,
    /// Audio backend (ASIO, WASAPI, ALSA, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

/// Device attachment status.
///
/// Represents the user-controlled lifecycle of a device in AudioMatrix.
/// By default, all devices are `Available` (zero auto-connect policy).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    /// Device detected but not attached to AudioMatrix.
    #[default]
    Available,
    /// User explicitly attached device, ready for routing.
    Attached,
    /// Device attached and actively streaming audio.
    Active,
    /// Device was attached but now detached.
    Detached,
    /// Device in error state.
    Error,
}

/// Device type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    /// Input device (capture only).
    Input,
    /// Output device (playback only).
    Output,
    /// Full-duplex device (both input and output).
    Duplex,
}

/// Request to attach a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachDeviceRequest {
    /// Optional display name for the device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Response after attaching a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachDeviceResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Updated device info.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Request to update a device (rename, configure sample rate/buffer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDeviceRequest {
    /// New display name (None to keep current, empty string to clear).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// New sample rate (None to keep current).
    /// Only 44100, 48000, 96000 Hz are supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// New buffer size (None to keep current).
    /// Must be power of 2: 32, 64, 128, 256, 512, 1024, 2048.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<u32>,
}

/// Channel information for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Channel number (1-based).
    pub number: u16,
    /// Channel label (user-defined).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Current level in dBFS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_dbfs: Option<f32>,
    /// Peak level in dBFS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_dbfs: Option<f32>,
}

/// Request to update a channel label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelLabelRequest {
    /// New label (empty string to clear).
    pub label: String,
}

/// Request to update multiple channel labels at once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkChannelLabelsRequest {
    /// Map of channel number (1-based) to label.
    pub labels: std::collections::HashMap<u16, String>,
}

/// Request to create a virtual device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVirtualDeviceRequest {
    /// Device name (required).
    pub name: String,
    /// Number of input channels (0-256).
    #[serde(default)]
    pub input_channels: u16,
    /// Number of output channels (0-256).
    #[serde(default)]
    pub output_channels: u16,
    /// Sample rate (44100, 48000, or 96000 only).
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// Buffer size in samples.
    #[serde(default = "default_buffer_size")]
    pub buffer_size: u32,
}

fn default_sample_rate() -> u32 {
    48000
}

fn default_buffer_size() -> u32 {
    256
}

/// Request to update a virtual device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVirtualDeviceRequest {
    /// New name (None to keep current).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New input channel count (None to keep current).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_channels: Option<u16>,
    /// New output channel count (None to keep current).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_channels: Option<u16>,
    /// New sample rate (None to keep current).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// New buffer size (None to keep current).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<u32>,
}

/// Response after creating/updating a virtual device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualDeviceResponse {
    /// Whether the operation succeeded.
    pub success: bool,
    /// Created/updated device info.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

/// Subscription request (sent from destination node to source node).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// Stream name for VBAN.
    pub stream_name: String,
    /// Source device on this node.
    pub source_device: String,
    /// Source channels (1-based).
    pub source_channels: Vec<u16>,
    /// Destination node name.
    pub destination_node: String,
    /// Destination address (IP:port for VBAN).
    pub destination_addr: String,
    /// Requested sample rate.
    pub sample_rate: u32,
}

/// Subscription response (sent from source node to destination node).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    /// Whether the subscription was accepted.
    pub success: bool,
    /// Subscription ID on source node.
    pub subscription_id: Option<u64>,
    /// VBAN stream name (may differ from requested).
    pub vban_stream_name: Option<String>,
    /// Actual sample rate.
    pub sample_rate: Option<u32>,
    /// Error message if failed.
    pub error: Option<String>,
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

// ============================================================================
// Wave Generator API Models
// ============================================================================

/// Waveform type for test signal generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaveformType {
    /// Sine wave at specified frequency.
    #[default]
    Sine,
    /// Pink noise (1/f spectrum).
    PinkNoise,
    /// Channel ID tone - unique frequency per channel.
    ChannelId,
    /// Frequency sweep (20Hz - 20kHz).
    Sweep,
    /// Click/impulse (single sample pulse).
    Click,
}

impl WaveformType {
    /// Convert to/from core type.
    #[cfg(feature = "server")]
    pub fn to_core(self) -> ram_core::WaveformType {
        match self {
            Self::Sine => ram_core::WaveformType::Sine,
            Self::PinkNoise => ram_core::WaveformType::PinkNoise,
            Self::ChannelId => ram_core::WaveformType::ChannelId,
            Self::Sweep => ram_core::WaveformType::Sweep,
            Self::Click => ram_core::WaveformType::Click,
        }
    }

    /// Convert from core type.
    #[cfg(feature = "server")]
    pub fn from_core(core: ram_core::WaveformType) -> Self {
        match core {
            ram_core::WaveformType::Sine => Self::Sine,
            ram_core::WaveformType::PinkNoise => Self::PinkNoise,
            ram_core::WaveformType::ChannelId => Self::ChannelId,
            ram_core::WaveformType::Sweep => Self::Sweep,
            ram_core::WaveformType::Click => Self::Click,
        }
    }
}

/// Request to set wave generator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetGeneratorRequest {
    /// Whether the generator is enabled.
    pub enabled: bool,
    /// Waveform type.
    #[serde(default)]
    pub waveform: WaveformType,
    /// Frequency in Hz (for sine, ignored for other waveforms).
    #[serde(default = "default_frequency")]
    pub frequency: u32,
    /// Output level in dBFS (clamped to -6.0..0.0).
    #[serde(default = "default_level")]
    pub level_db: f32,
}

fn default_frequency() -> u32 {
    1000
}

fn default_level() -> f32 {
    -18.0
}

/// Request to set generator for all channels of a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAllGeneratorsRequest {
    /// Whether generators are enabled.
    pub enabled: bool,
    /// Waveform type.
    #[serde(default)]
    pub waveform: WaveformType,
    /// Frequency in Hz (for sine, ignored for other waveforms).
    #[serde(default = "default_frequency")]
    pub frequency: u32,
    /// Output level in dBFS (clamped to -6.0..0.0).
    #[serde(default = "default_level")]
    pub level_db: f32,
}

/// Wave generator status for a single channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorStatus {
    /// Channel number (1-based).
    pub channel: u16,
    /// Whether the generator is enabled.
    pub enabled: bool,
    /// Current waveform type.
    pub waveform: WaveformType,
    /// Frequency in Hz.
    pub frequency: u32,
    /// Output level in dBFS.
    pub level_db: f32,
}

/// Response containing generator status for a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGeneratorResponse {
    /// Device identifier.
    pub device_id: String,
    /// Generator status for each output channel.
    pub channels: Vec<GeneratorStatus>,
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
            display_name: None,
            device_type: DeviceType::Input,
            input_channels: 2,
            output_channels: 0,
            sample_rate: 48000,
            buffer_size: 256,
            is_virtual: false,
            status: DeviceStatus::Available,
            backend: None,
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
            display_name: Some("Main Out".into()),
            device_type: DeviceType::Output,
            input_channels: 0,
            output_channels: 8,
            sample_rate: 96000,
            buffer_size: 128,
            is_virtual: true,
            status: DeviceStatus::Attached,
            backend: Some("ASIO".into()),
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
            stream_name: "AM_TEST_1".into(),
            source_device: "input-device".into(),
            source_channels: vec![1, 2, 3],
            destination_node: "remote-node".into(),
            destination_addr: "192.168.1.100:6980".into(),
            sample_rate: 48000,
        };

        let json = serde_json::to_string(&sub).unwrap();
        let parsed: SubscriptionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.stream_name, "AM_TEST_1");
        assert_eq!(parsed.source_device, "input-device");
        assert_eq!(parsed.source_channels, vec![1, 2, 3]);
        assert_eq!(parsed.destination_node, "remote-node");
        assert_eq!(parsed.sample_rate, 48000);
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
