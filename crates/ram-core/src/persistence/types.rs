//! Persisted data types.
//!
//! This module contains all the data structures used for configuration
//! persistence.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Persisted device attachment state.
///
/// Stores which devices were attached before shutdown so they can be
/// restored on startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDevice {
    /// Device identifier.
    pub id: String,
    /// User-defined display name (alias).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Whether the device was attached (should be re-attached on startup).
    #[serde(default)]
    pub attached: bool,
}

/// Channel labels for a device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelLabels {
    /// Device identifier.
    pub device_id: String,
    /// Map of channel number (1-based) to label.
    #[serde(default)]
    pub labels: HashMap<u16, String>,
}

impl ChannelLabels {
    /// Maximum label length (31 characters for VBAN compatibility).
    pub const MAX_LABEL_LENGTH: usize = 31;

    /// Creates new channel labels for a device.
    #[must_use]
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            labels: HashMap::new(),
        }
    }

    /// Sets a label for a channel.
    ///
    /// Returns true if the label was set, false if it was too long.
    pub fn set_label(&mut self, channel: u16, label: String) -> bool {
        if label.len() > Self::MAX_LABEL_LENGTH {
            return false;
        }
        if label.is_empty() {
            self.labels.remove(&channel);
        } else {
            self.labels.insert(channel, label);
        }
        true
    }

    /// Gets the label for a channel.
    #[must_use]
    pub fn get_label(&self, channel: u16) -> Option<&String> {
        self.labels.get(&channel)
    }

    /// Returns all labels.
    #[must_use]
    pub fn all_labels(&self) -> &HashMap<u16, String> {
        &self.labels
    }
}

/// Virtual device configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualDeviceConfig {
    /// Virtual device name.
    pub name: String,
    /// Number of input channels.
    #[serde(default)]
    pub input_channels: u16,
    /// Number of output channels.
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

impl VirtualDeviceConfig {
    /// Supported sample rates.
    pub const SUPPORTED_SAMPLE_RATES: [u32; 3] = [44100, 48000, 96000];

    /// Validates the configuration.
    ///
    /// Returns true if the configuration is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        Self::SUPPORTED_SAMPLE_RATES.contains(&self.sample_rate)
            && (self.input_channels > 0 || self.output_channels > 0)
            && self.input_channels <= 256
            && self.output_channels <= 256
    }
}

/// Route definition for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRoute {
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

/// Node configuration for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedNode {
    /// Node name.
    pub name: String,
    /// API port.
    pub api_port: u16,
    /// VBAN port.
    pub vban_port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_labels_basic() {
        let mut labels = ChannelLabels::new("device-1".into());
        assert!(labels.set_label(1, "Mic L".into()));
        assert!(labels.set_label(2, "Mic R".into()));

        assert_eq!(labels.get_label(1), Some(&"Mic L".to_string()));
        assert_eq!(labels.get_label(2), Some(&"Mic R".to_string()));
        assert_eq!(labels.get_label(3), None);
    }

    #[test]
    fn channel_labels_max_length() {
        let mut labels = ChannelLabels::new("device-1".into());

        // 31 chars should work
        let label_31 = "a".repeat(31);
        assert!(labels.set_label(1, label_31.clone()));
        assert_eq!(labels.get_label(1), Some(&label_31));

        // 32 chars should fail
        let label_32 = "a".repeat(32);
        assert!(!labels.set_label(2, label_32));
        assert_eq!(labels.get_label(2), None);
    }

    #[test]
    fn channel_labels_empty_removes() {
        let mut labels = ChannelLabels::new("device-1".into());
        labels.set_label(1, "Test".into());
        assert!(labels.get_label(1).is_some());

        labels.set_label(1, String::new());
        assert!(labels.get_label(1).is_none());
    }

    #[test]
    fn virtual_device_config_valid() {
        let config = VirtualDeviceConfig {
            name: "Virtual In".into(),
            input_channels: 8,
            output_channels: 0,
            sample_rate: 48000,
            buffer_size: 256,
        };
        assert!(config.is_valid());
    }

    #[test]
    fn virtual_device_config_invalid_sample_rate() {
        let config = VirtualDeviceConfig {
            name: "Bad Rate".into(),
            input_channels: 2,
            output_channels: 0,
            sample_rate: 22050, // Not supported
            buffer_size: 256,
        };
        assert!(!config.is_valid());
    }

    #[test]
    fn virtual_device_config_invalid_channels() {
        // Zero channels
        let config = VirtualDeviceConfig {
            name: "No Channels".into(),
            input_channels: 0,
            output_channels: 0,
            sample_rate: 48000,
            buffer_size: 256,
        };
        assert!(!config.is_valid());
    }
}
