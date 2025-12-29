//! Persisted configuration structure.
//!
//! This module contains the main `PersistedConfig` struct that holds
//! all configuration state.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::{
    ChannelLabels, PersistedDevice, PersistedNode, PersistedRoute, VirtualDeviceConfig,
};

fn default_version() -> u32 {
    1
}

/// Full configuration state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedConfig {
    /// Configuration version for migrations.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Node configuration.
    pub node: Option<PersistedNode>,
    /// Active routes.
    #[serde(default)]
    pub routes: Vec<PersistedRoute>,
    /// Attached devices (to restore on startup).
    #[serde(default)]
    pub attached_devices: Vec<PersistedDevice>,
    /// Channel labels by device ID.
    #[serde(default)]
    pub channel_labels: HashMap<String, ChannelLabels>,
    /// Virtual device configurations.
    #[serde(default)]
    pub virtual_devices: Vec<VirtualDeviceConfig>,
    /// Custom key-value settings.
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

impl PersistedConfig {
    /// Creates a new empty configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            node: None,
            routes: Vec::new(),
            attached_devices: Vec::new(),
            channel_labels: HashMap::new(),
            virtual_devices: Vec::new(),
            settings: HashMap::new(),
        }
    }

    /// Creates configuration with node settings.
    #[must_use]
    pub fn with_node(name: String, api_port: u16, vban_port: u16) -> Self {
        Self {
            version: 1,
            node: Some(PersistedNode {
                name,
                api_port,
                vban_port,
            }),
            routes: Vec::new(),
            attached_devices: Vec::new(),
            channel_labels: HashMap::new(),
            virtual_devices: Vec::new(),
            settings: HashMap::new(),
        }
    }

    /// Records a device as attached.
    pub fn attach_device(&mut self, id: String, display_name: Option<String>) {
        // Update or add device
        if let Some(dev) = self.attached_devices.iter_mut().find(|d| d.id == id) {
            dev.attached = true;
            if display_name.is_some() {
                dev.display_name = display_name;
            }
        } else {
            self.attached_devices.push(PersistedDevice {
                id,
                display_name,
                attached: true,
            });
        }
    }

    /// Records a device as detached.
    pub fn detach_device(&mut self, id: &str) {
        if let Some(dev) = self.attached_devices.iter_mut().find(|d| d.id == id) {
            dev.attached = false;
        }
    }

    /// Returns attached device IDs.
    #[must_use]
    pub fn attached_device_ids(&self) -> Vec<&str> {
        self.attached_devices
            .iter()
            .filter(|d| d.attached)
            .map(|d| d.id.as_str())
            .collect()
    }

    /// Sets a channel label for a device.
    pub fn set_channel_label(&mut self, device_id: &str, channel: u16, label: String) -> bool {
        let labels = self
            .channel_labels
            .entry(device_id.to_string())
            .or_insert_with(|| ChannelLabels::new(device_id.to_string()));
        labels.set_label(channel, label)
    }

    /// Gets channel labels for a device.
    #[must_use]
    pub fn get_channel_labels(&self, device_id: &str) -> Option<&ChannelLabels> {
        self.channel_labels.get(device_id)
    }

    /// Adds a virtual device configuration.
    pub fn add_virtual_device(&mut self, config: VirtualDeviceConfig) {
        if config.is_valid() {
            self.virtual_devices.push(config);
        }
    }

    /// Removes a virtual device configuration by name.
    pub fn remove_virtual_device(&mut self, name: &str) -> bool {
        let len = self.virtual_devices.len();
        self.virtual_devices.retain(|v| v.name != name);
        self.virtual_devices.len() != len
    }

    /// Adds a route to the configuration.
    pub fn add_route(&mut self, route: PersistedRoute) {
        self.routes.push(route);
    }

    /// Removes a route by matching source and destination.
    #[allow(clippy::too_many_arguments)]
    pub fn remove_route(
        &mut self,
        source_node: &str,
        source_device: &str,
        source_channel: u16,
        dest_node: &str,
        dest_device: &str,
        dest_channel: u16,
    ) -> bool {
        let initial_len = self.routes.len();
        self.routes.retain(|r| {
            !(r.source_node == source_node
                && r.source_device == source_device
                && r.source_channel == source_channel
                && r.destination_node == dest_node
                && r.destination_device == dest_device
                && r.destination_channel == dest_channel)
        });
        self.routes.len() != initial_len
    }

    /// Sets a custom setting.
    pub fn set_setting(&mut self, key: String, value: String) {
        self.settings.insert(key, value);
    }

    /// Gets a custom setting.
    #[must_use]
    pub fn get_setting(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_config_new() {
        let config = PersistedConfig::new();
        assert_eq!(config.version, 1);
        assert!(config.node.is_none());
        assert!(config.routes.is_empty());
        assert!(config.settings.is_empty());
    }

    #[test]
    fn persisted_config_with_node() {
        let config = PersistedConfig::with_node("TestNode".into(), 8080, 6980);
        let node = config.node.unwrap();
        assert_eq!(node.name, "TestNode");
        assert_eq!(node.api_port, 8080);
        assert_eq!(node.vban_port, 6980);
    }

    #[test]
    fn persisted_config_add_route() {
        let mut config = PersistedConfig::new();
        config.add_route(PersistedRoute {
            source_node: "node-a".into(),
            source_device: "dev-1".into(),
            source_channel: 1,
            destination_node: "node-b".into(),
            destination_device: "dev-2".into(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        });

        assert_eq!(config.routes.len(), 1);
    }

    #[test]
    fn persisted_config_remove_route() {
        let mut config = PersistedConfig::new();
        config.add_route(PersistedRoute {
            source_node: "node-a".into(),
            source_device: "dev-1".into(),
            source_channel: 1,
            destination_node: "node-b".into(),
            destination_device: "dev-2".into(),
            destination_channel: 1,
            volume: 1.0,
            muted: false,
        });

        let removed = config.remove_route("node-a", "dev-1", 1, "node-b", "dev-2", 1);
        assert!(removed);
        assert!(config.routes.is_empty());

        let removed_again = config.remove_route("node-a", "dev-1", 1, "node-b", "dev-2", 1);
        assert!(!removed_again);
    }

    #[test]
    fn persisted_config_settings() {
        let mut config = PersistedConfig::new();
        config.set_setting("key1".into(), "value1".into());

        assert_eq!(config.get_setting("key1"), Some(&"value1".to_string()));
        assert_eq!(config.get_setting("nonexistent"), None);
    }

    #[test]
    fn persisted_config_serialization() {
        let mut config = PersistedConfig::with_node("Test".into(), 8080, 6980);
        config.add_route(PersistedRoute {
            source_node: "a".into(),
            source_device: "d1".into(),
            source_channel: 1,
            destination_node: "b".into(),
            destination_device: "d2".into(),
            destination_channel: 2,
            volume: 0.8,
            muted: true,
        });
        config.set_setting("test".into(), "value".into());

        let json = serde_json::to_string(&config).unwrap();
        let parsed: PersistedConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert!(parsed.node.is_some());
        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(parsed.settings.len(), 1);
    }

    #[test]
    fn persisted_config_device_attachment() {
        let mut config = PersistedConfig::new();

        config.attach_device("dev-1".into(), Some("Main Mic".into()));
        config.attach_device("dev-2".into(), None);

        let attached = config.attached_device_ids();
        assert_eq!(attached.len(), 2);
        assert!(attached.contains(&"dev-1"));
        assert!(attached.contains(&"dev-2"));

        config.detach_device("dev-1");
        let attached = config.attached_device_ids();
        assert_eq!(attached.len(), 1);
        assert!(!attached.contains(&"dev-1"));
    }

    #[test]
    fn persisted_config_channel_labels() {
        let mut config = PersistedConfig::new();

        assert!(config.set_channel_label("dev-1", 1, "Kick".into()));
        assert!(config.set_channel_label("dev-1", 2, "Snare".into()));

        let labels = config.get_channel_labels("dev-1").unwrap();
        assert_eq!(labels.get_label(1), Some(&"Kick".to_string()));
        assert_eq!(labels.get_label(2), Some(&"Snare".to_string()));
    }

    #[test]
    fn persisted_config_virtual_devices() {
        let mut config = PersistedConfig::new();

        config.add_virtual_device(VirtualDeviceConfig {
            name: "Virtual In".into(),
            input_channels: 16,
            output_channels: 0,
            sample_rate: 48000,
            buffer_size: 64,
        });

        assert_eq!(config.virtual_devices.len(), 1);

        let removed = config.remove_virtual_device("Virtual In");
        assert!(removed);
        assert!(config.virtual_devices.is_empty());
    }

    #[test]
    fn persisted_config_serialization_with_new_fields() {
        let mut config = PersistedConfig::new();
        config.attach_device("dev-1".into(), Some("Mic".into()));
        config.set_channel_label("dev-1", 1, "Left".into());
        config.add_virtual_device(VirtualDeviceConfig {
            name: "Virtual".into(),
            input_channels: 2,
            output_channels: 2,
            sample_rate: 48000,
            buffer_size: 256,
        });

        let json = serde_json::to_string(&config).unwrap();
        let parsed: PersistedConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.attached_devices.len(), 1);
        assert_eq!(parsed.channel_labels.len(), 1);
        assert_eq!(parsed.virtual_devices.len(), 1);
    }
}
