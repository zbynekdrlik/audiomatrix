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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
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
