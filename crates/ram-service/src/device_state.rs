//! Device state persistence.
//!
//! Saves and loads device state (attached status, display names, channel labels,
//! configuration) to/from a JSON file for persistence across restarts.

use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

/// Persisted device state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedDeviceState {
    /// Device ID.
    pub id: String,
    /// Whether the device should be attached on startup.
    pub attached: bool,
    /// Custom display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Configured sample rate (if overridden).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// Configured buffer size (if overridden).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_size: Option<u32>,
}

/// Channel labels for a device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedChannelLabels {
    /// Device ID.
    pub device_id: String,
    /// Channel labels (channel number -> label).
    pub labels: HashMap<u16, String>,
}

/// Complete persisted state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceStateFile {
    /// Version for future migrations.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Device states.
    #[serde(default)]
    pub devices: Vec<PersistedDeviceState>,
    /// Channel labels.
    #[serde(default)]
    pub channel_labels: Vec<PersistedChannelLabels>,
}

fn default_version() -> u32 {
    1
}

/// Manager for device state persistence.
pub struct DeviceStateManager {
    /// Path to the state file.
    path: PathBuf,
    /// Cached state.
    state: RwLock<DeviceStateFile>,
}

impl DeviceStateManager {
    /// Creates a new device state manager.
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Directory to store the state file in.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir.join("device_state.json");
        let state = Self::load_from_file(&path).unwrap_or_default();

        Self {
            path,
            state: RwLock::new(state),
        }
    }

    /// Loads state from file.
    fn load_from_file(path: &PathBuf) -> Option<DeviceStateFile> {
        if !path.exists() {
            debug!("No device state file at {}", path.display());
            return None;
        }

        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(state) => {
                    info!("Loaded device state from {}", path.display());
                    Some(state)
                }
                Err(e) => {
                    error!("Failed to parse device state file: {}", e);
                    None
                }
            },
            Err(e) => {
                error!("Failed to read device state file: {}", e);
                None
            }
        }
    }

    /// Saves state to file.
    fn save_to_file(&self) -> Result<(), String> {
        let state = self.state.read();

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(format!("Failed to create directory: {e}"));
                }
            }
        }

        let content = serde_json::to_string_pretty(&*state)
            .map_err(|e| format!("Failed to serialize state: {e}"))?;

        std::fs::write(&self.path, content)
            .map_err(|e| format!("Failed to write state file: {e}"))?;

        debug!("Saved device state to {}", self.path.display());
        Ok(())
    }

    /// Gets the attached status for a device.
    #[must_use]
    pub fn is_device_attached(&self, device_id: &str) -> bool {
        let state = self.state.read();
        state
            .devices
            .iter()
            .find(|d| d.id == device_id)
            .map_or(false, |d| d.attached)
    }

    /// Gets the persisted state for a device.
    #[must_use]
    pub fn get_device_state(&self, device_id: &str) -> Option<PersistedDeviceState> {
        let state = self.state.read();
        state.devices.iter().find(|d| d.id == device_id).cloned()
    }

    /// Sets device attached status.
    pub fn set_device_attached(&self, device_id: &str, attached: bool) -> Result<(), String> {
        {
            let mut state = self.state.write();
            if let Some(device) = state.devices.iter_mut().find(|d| d.id == device_id) {
                device.attached = attached;
            } else {
                state.devices.push(PersistedDeviceState {
                    id: device_id.to_string(),
                    attached,
                    display_name: None,
                    sample_rate: None,
                    buffer_size: None,
                });
            }
        }
        self.save_to_file()
    }

    /// Sets device display name.
    pub fn set_device_display_name(
        &self,
        device_id: &str,
        display_name: Option<String>,
    ) -> Result<(), String> {
        {
            let mut state = self.state.write();
            if let Some(device) = state.devices.iter_mut().find(|d| d.id == device_id) {
                device.display_name = display_name;
            } else {
                state.devices.push(PersistedDeviceState {
                    id: device_id.to_string(),
                    attached: false,
                    display_name,
                    sample_rate: None,
                    buffer_size: None,
                });
            }
        }
        self.save_to_file()
    }

    /// Sets device configuration.
    pub fn set_device_config(
        &self,
        device_id: &str,
        sample_rate: Option<u32>,
        buffer_size: Option<u32>,
    ) -> Result<(), String> {
        {
            let mut state = self.state.write();
            if let Some(device) = state.devices.iter_mut().find(|d| d.id == device_id) {
                if sample_rate.is_some() {
                    device.sample_rate = sample_rate;
                }
                if buffer_size.is_some() {
                    device.buffer_size = buffer_size;
                }
            } else {
                state.devices.push(PersistedDeviceState {
                    id: device_id.to_string(),
                    attached: false,
                    display_name: None,
                    sample_rate,
                    buffer_size,
                });
            }
        }
        self.save_to_file()
    }

    /// Gets channel labels for a device.
    #[must_use]
    pub fn get_channel_labels(&self, device_id: &str) -> HashMap<u16, String> {
        let state = self.state.read();
        state
            .channel_labels
            .iter()
            .find(|l| l.device_id == device_id)
            .map_or_else(HashMap::new, |l| l.labels.clone())
    }

    /// Sets a channel label.
    pub fn set_channel_label(
        &self,
        device_id: &str,
        channel: u16,
        label: String,
    ) -> Result<(), String> {
        {
            let mut state = self.state.write();
            if let Some(labels) = state
                .channel_labels
                .iter_mut()
                .find(|l| l.device_id == device_id)
            {
                if label.is_empty() {
                    labels.labels.remove(&channel);
                } else {
                    labels.labels.insert(channel, label);
                }
            } else if !label.is_empty() {
                let mut labels_map = HashMap::new();
                labels_map.insert(channel, label);
                state.channel_labels.push(PersistedChannelLabels {
                    device_id: device_id.to_string(),
                    labels: labels_map,
                });
            }
        }
        self.save_to_file()
    }

    /// Sets multiple channel labels.
    pub fn set_channel_labels(
        &self,
        device_id: &str,
        labels_map: HashMap<u16, String>,
    ) -> Result<(), String> {
        {
            let mut state = self.state.write();
            if let Some(labels) = state
                .channel_labels
                .iter_mut()
                .find(|l| l.device_id == device_id)
            {
                for (channel, label) in labels_map {
                    if label.is_empty() {
                        labels.labels.remove(&channel);
                    } else {
                        labels.labels.insert(channel, label);
                    }
                }
            } else {
                let filtered: HashMap<u16, String> =
                    labels_map.into_iter().filter(|(_, v)| !v.is_empty()).collect();
                if !filtered.is_empty() {
                    state.channel_labels.push(PersistedChannelLabels {
                        device_id: device_id.to_string(),
                        labels: filtered,
                    });
                }
            }
        }
        self.save_to_file()
    }

    /// Gets all devices that should be attached on startup.
    #[must_use]
    pub fn get_attached_devices(&self) -> Vec<String> {
        let state = self.state.read();
        state
            .devices
            .iter()
            .filter(|d| d.attached)
            .map(|d| d.id.clone())
            .collect()
    }

    /// Gets all persisted device states.
    #[must_use]
    pub fn get_all_device_states(&self) -> Vec<PersistedDeviceState> {
        self.state.read().devices.clone()
    }

    /// Gets all channel labels.
    #[must_use]
    pub fn get_all_channel_labels(&self) -> HashMap<String, HashMap<u16, String>> {
        let state = self.state.read();
        state
            .channel_labels
            .iter()
            .map(|l| (l.device_id.clone(), l.labels.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (DeviceStateManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = DeviceStateManager::new(temp_dir.path().to_path_buf());
        (manager, temp_dir)
    }

    #[test]
    fn test_new_manager_empty_state() {
        let (manager, _temp) = create_test_manager();
        assert!(manager.get_attached_devices().is_empty());
    }

    #[test]
    fn test_set_device_attached() {
        let (manager, _temp) = create_test_manager();

        manager.set_device_attached("device1", true).unwrap();
        assert!(manager.is_device_attached("device1"));

        manager.set_device_attached("device1", false).unwrap();
        assert!(!manager.is_device_attached("device1"));
    }

    #[test]
    fn test_set_device_display_name() {
        let (manager, _temp) = create_test_manager();

        manager
            .set_device_display_name("device1", Some("My Device".to_string()))
            .unwrap();

        let state = manager.get_device_state("device1").unwrap();
        assert_eq!(state.display_name, Some("My Device".to_string()));
    }

    #[test]
    fn test_set_device_config() {
        let (manager, _temp) = create_test_manager();

        manager
            .set_device_config("device1", Some(48000), Some(256))
            .unwrap();

        let state = manager.get_device_state("device1").unwrap();
        assert_eq!(state.sample_rate, Some(48000));
        assert_eq!(state.buffer_size, Some(256));
    }

    #[test]
    fn test_channel_labels() {
        let (manager, _temp) = create_test_manager();

        manager
            .set_channel_label("device1", 1, "Kick".to_string())
            .unwrap();
        manager
            .set_channel_label("device1", 2, "Snare".to_string())
            .unwrap();

        let labels = manager.get_channel_labels("device1");
        assert_eq!(labels.get(&1), Some(&"Kick".to_string()));
        assert_eq!(labels.get(&2), Some(&"Snare".to_string()));
    }

    #[test]
    fn test_persistence_across_loads() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // Create and populate state
        {
            let manager = DeviceStateManager::new(path.clone());
            manager.set_device_attached("device1", true).unwrap();
            manager
                .set_device_display_name("device1", Some("My Device".to_string()))
                .unwrap();
            manager
                .set_channel_label("device1", 1, "Kick".to_string())
                .unwrap();
        }

        // Load again and verify
        {
            let manager = DeviceStateManager::new(path);
            assert!(manager.is_device_attached("device1"));

            let state = manager.get_device_state("device1").unwrap();
            assert_eq!(state.display_name, Some("My Device".to_string()));

            let labels = manager.get_channel_labels("device1");
            assert_eq!(labels.get(&1), Some(&"Kick".to_string()));
        }
    }

    #[test]
    fn test_get_attached_devices() {
        let (manager, _temp) = create_test_manager();

        manager.set_device_attached("device1", true).unwrap();
        manager.set_device_attached("device2", false).unwrap();
        manager.set_device_attached("device3", true).unwrap();

        let attached = manager.get_attached_devices();
        assert_eq!(attached.len(), 2);
        assert!(attached.contains(&"device1".to_string()));
        assert!(attached.contains(&"device3".to_string()));
    }
}
