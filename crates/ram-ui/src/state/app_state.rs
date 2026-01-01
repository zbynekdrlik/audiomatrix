//! Global application state.

use leptos::prelude::*;
use ram_api::models::{DeviceInfo, NodeInfo, RouteDefinition};

/// Route with ID for display purposes.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteWithId {
    pub id: String,
    pub source_node: String,
    pub source_device: String,
    pub source_channel: u16,
    pub destination_node: String,
    pub destination_device: String,
    pub destination_channel: u16,
    pub volume: f32,
    pub muted: bool,
}

impl From<RouteDefinition> for RouteWithId {
    fn from(route: RouteDefinition) -> Self {
        let id = format!(
            "{}:{}:{}->{}:{}:{}",
            route.source_node,
            route.source_device,
            route.source_channel,
            route.destination_node,
            route.destination_device,
            route.destination_channel
        );
        Self {
            id,
            source_node: route.source_node,
            source_device: route.source_device,
            source_channel: route.source_channel,
            destination_node: route.destination_node,
            destination_device: route.destination_device,
            destination_channel: route.destination_channel,
            volume: route.volume,
            muted: route.muted,
        }
    }
}

/// Metering data for a device channel.
#[derive(Debug, Clone, Default)]
pub struct ChannelLevel {
    /// Current level in dBFS.
    pub level_db: f32,
    /// Peak hold level in dBFS.
    pub peak_db: f32,
}

/// Global application state.
///
/// This is provided at the app root and accessible from any component.
#[derive(Clone)]
pub struct AppState {
    /// Current node (the one we're connected to).
    pub current_node: RwSignal<Option<NodeInfo>>,

    /// All discovered nodes.
    pub nodes: RwSignal<Vec<NodeInfo>>,

    /// Devices for the current node.
    pub devices: RwSignal<Vec<DeviceInfo>>,

    /// Active routes.
    pub routes: RwSignal<Vec<RouteWithId>>,

    /// Input device metering levels (device_id -> channel levels).
    pub input_levels: RwSignal<std::collections::HashMap<String, Vec<ChannelLevel>>>,

    /// Output device metering levels (device_id -> channel levels).
    pub output_levels: RwSignal<std::collections::HashMap<String, Vec<ChannelLevel>>>,

    /// Connection status.
    pub connected: RwSignal<bool>,

    /// Loading state.
    pub loading: RwSignal<bool>,

    /// Error message (if any).
    pub error: RwSignal<Option<String>>,

    /// Selected source device for routing.
    pub selected_source: RwSignal<Option<String>>,

    /// Selected destination device for routing.
    pub selected_destination: RwSignal<Option<String>>,

    /// Selected source node for cross-node routing.
    pub source_node: RwSignal<Option<NodeInfo>>,

    /// Selected destination node for cross-node routing.
    pub dest_node: RwSignal<Option<NodeInfo>>,

    /// Devices from the selected source node.
    pub source_devices: RwSignal<Vec<DeviceInfo>>,

    /// Devices from the selected destination node.
    pub dest_devices: RwSignal<Vec<DeviceInfo>>,
}

impl AppState {
    /// Creates a new application state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_node: RwSignal::new(None),
            nodes: RwSignal::new(Vec::new()),
            devices: RwSignal::new(Vec::new()),
            routes: RwSignal::new(Vec::new()),
            input_levels: RwSignal::new(std::collections::HashMap::new()),
            output_levels: RwSignal::new(std::collections::HashMap::new()),
            connected: RwSignal::new(false),
            loading: RwSignal::new(true),
            error: RwSignal::new(None),
            selected_source: RwSignal::new(None),
            selected_destination: RwSignal::new(None),
            source_node: RwSignal::new(None),
            dest_node: RwSignal::new(None),
            source_devices: RwSignal::new(Vec::new()),
            dest_devices: RwSignal::new(Vec::new()),
        }
    }

    /// Sets the current node and triggers data refresh.
    pub fn set_current_node(&self, node: NodeInfo) {
        self.current_node.set(Some(node));
    }

    /// Adds or updates a route.
    pub fn upsert_route(&self, route: RouteDefinition) {
        let route_with_id = RouteWithId::from(route);
        self.routes.update(|routes| {
            if let Some(existing) = routes.iter_mut().find(|r| r.id == route_with_id.id) {
                *existing = route_with_id;
            } else {
                routes.push(route_with_id);
            }
        });
    }

    /// Removes a route by ID.
    pub fn remove_route(&self, route_id: &str) {
        self.routes.update(|routes| {
            routes.retain(|r| r.id != route_id);
        });
    }

    /// Updates metering levels for a device.
    pub fn update_levels(&self, device_id: &str, is_input: bool, levels: Vec<ChannelLevel>) {
        let signal = if is_input {
            &self.input_levels
        } else {
            &self.output_levels
        };
        signal.update(|map| {
            map.insert(device_id.to_string(), levels);
        });
    }

    /// Gets input devices (attached/active only, including duplex devices with input channels).
    #[must_use]
    pub fn input_devices(&self) -> Vec<DeviceInfo> {
        self.devices
            .get()
            .into_iter()
            .filter(|d| {
                // Only show attached or active devices
                let is_attached = matches!(
                    d.status,
                    ram_api::models::DeviceStatus::Attached | ram_api::models::DeviceStatus::Active
                );
                // Include input devices OR duplex devices with input channels
                let has_input = matches!(d.device_type, ram_api::models::DeviceType::Input)
                    || (matches!(d.device_type, ram_api::models::DeviceType::Duplex)
                        && d.input_channels > 0);
                is_attached && has_input
            })
            .collect()
    }

    /// Gets output devices (attached/active only, including duplex devices with output channels).
    #[must_use]
    pub fn output_devices(&self) -> Vec<DeviceInfo> {
        self.devices
            .get()
            .into_iter()
            .filter(|d| {
                // Only show attached or active devices
                let is_attached = matches!(
                    d.status,
                    ram_api::models::DeviceStatus::Attached | ram_api::models::DeviceStatus::Active
                );
                // Include output devices OR duplex devices with output channels
                let has_output = matches!(d.device_type, ram_api::models::DeviceType::Output)
                    || (matches!(d.device_type, ram_api::models::DeviceType::Duplex)
                        && d.output_channels > 0);
                is_attached && has_output
            })
            .collect()
    }

    /// Gets input devices from the source node (for cross-node routing).
    /// Filters to only show attached/active devices with input capability.
    #[must_use]
    pub fn source_input_devices(&self) -> Vec<DeviceInfo> {
        self.source_devices
            .get()
            .into_iter()
            .filter(|d| {
                // Only show attached or active devices
                let is_attached = matches!(
                    d.status,
                    ram_api::models::DeviceStatus::Attached | ram_api::models::DeviceStatus::Active
                );
                // Include input devices OR duplex devices with input channels
                let has_input = matches!(d.device_type, ram_api::models::DeviceType::Input)
                    || (matches!(d.device_type, ram_api::models::DeviceType::Duplex)
                        && d.input_channels > 0);
                is_attached && has_input
            })
            .collect()
    }

    /// Gets output devices from the destination node (for cross-node routing).
    /// Filters to only show attached/active devices with output capability.
    #[must_use]
    pub fn dest_output_devices(&self) -> Vec<DeviceInfo> {
        self.dest_devices
            .get()
            .into_iter()
            .filter(|d| {
                // Only show attached or active devices
                let is_attached = matches!(
                    d.status,
                    ram_api::models::DeviceStatus::Attached | ram_api::models::DeviceStatus::Active
                );
                // Include output devices OR duplex devices with output channels
                let has_output = matches!(d.device_type, ram_api::models::DeviceType::Output)
                    || (matches!(d.device_type, ram_api::models::DeviceType::Duplex)
                        && d.output_channels > 0);
                is_attached && has_output
            })
            .collect()
    }

    /// Gets the source node ID or "LOCAL" if not set.
    #[must_use]
    pub fn source_node_id(&self) -> String {
        self.source_node
            .get()
            .map(|n| n.id)
            .unwrap_or_else(|| "LOCAL".to_string())
    }

    /// Gets the destination node ID or "LOCAL" if not set.
    #[must_use]
    pub fn dest_node_id(&self) -> String {
        self.dest_node
            .get()
            .map(|n| n.id)
            .unwrap_or_else(|| "LOCAL".to_string())
    }

    /// Checks if a route exists between source and destination channels.
    #[must_use]
    pub fn has_route(
        &self,
        source_device: &str,
        source_ch: u16,
        dest_device: &str,
        dest_ch: u16,
    ) -> bool {
        self.routes.get().iter().any(|r| {
            r.source_device == source_device
                && r.source_channel == source_ch
                && r.destination_device == dest_device
                && r.destination_channel == dest_ch
        })
    }

    /// Gets the route between source and destination channels.
    #[must_use]
    pub fn get_route(
        &self,
        source_device: &str,
        source_ch: u16,
        dest_device: &str,
        dest_ch: u16,
    ) -> Option<RouteWithId> {
        self.routes.get().into_iter().find(|r| {
            r.source_device == source_device
                && r.source_channel == source_ch
                && r.destination_device == dest_device
                && r.destination_channel == dest_ch
        })
    }

    /// Checks if a route exists between source and destination with node info.
    #[must_use]
    pub fn has_route_with_nodes(
        &self,
        source_node: &str,
        source_device: &str,
        source_ch: u16,
        dest_node: &str,
        dest_device: &str,
        dest_ch: u16,
    ) -> bool {
        self.routes.get().iter().any(|r| {
            r.source_node == source_node
                && r.source_device == source_device
                && r.source_channel == source_ch
                && r.destination_node == dest_node
                && r.destination_device == dest_device
                && r.destination_channel == dest_ch
        })
    }

    /// Gets the route between source and destination with node info.
    #[must_use]
    pub fn get_route_with_nodes(
        &self,
        source_node: &str,
        source_device: &str,
        source_ch: u16,
        dest_node: &str,
        dest_device: &str,
        dest_ch: u16,
    ) -> Option<RouteWithId> {
        self.routes.get().into_iter().find(|r| {
            r.source_node == source_node
                && r.source_device == source_device
                && r.source_channel == source_ch
                && r.destination_node == dest_node
                && r.destination_device == dest_device
                && r.destination_channel == dest_ch
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_with_id_from_definition() {
        let route = RouteDefinition {
            source_node: "LOCAL".into(),
            source_device: "mic".into(),
            source_channel: 1,
            destination_node: "LOCAL".into(),
            destination_device: "speakers".into(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        };

        let route_with_id = RouteWithId::from(route);
        assert_eq!(route_with_id.id, "LOCAL:mic:1->LOCAL:speakers:1");
        assert_eq!(route_with_id.volume, 1.0);
    }
}
