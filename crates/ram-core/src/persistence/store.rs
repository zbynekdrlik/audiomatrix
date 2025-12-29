//! Configuration store for file-based persistence.
//!
//! This module provides the `ConfigStore` which manages loading and saving
//! configuration to disk.

use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use super::config::PersistedConfig;
use super::types::{ChannelLabels, PersistedDevice, PersistedRoute, VirtualDeviceConfig};
use super::{PersistenceError, Result};

/// Manages configuration persistence.
pub struct ConfigStore {
    /// Configuration file path.
    path: PathBuf,
    /// Current configuration.
    config: RwLock<PersistedConfig>,
    /// Auto-save enabled.
    auto_save: bool,
}

impl ConfigStore {
    /// Creates a new configuration store at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be loaded.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let config = if path.exists() {
            Self::load_from_file(&path)?
        } else {
            PersistedConfig::new()
        };

        Ok(Self {
            path,
            config: RwLock::new(config),
            auto_save: true,
        })
    }

    /// Creates a store with default platform-specific location.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration directory cannot be found.
    pub fn with_default_path() -> Result<Self> {
        let config_dir = Self::default_config_dir()?;
        fs::create_dir_all(&config_dir)?;
        let path = config_dir.join("config.json");
        Self::new(path)
    }

    /// Returns the default configuration directory.
    fn default_config_dir() -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::env::var("APPDATA")
                .map(|p| PathBuf::from(p).join("AudioMatrix"))
                .map_err(|_| PersistenceError::ConfigDirNotFound)
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var("HOME")
                .map(|p| PathBuf::from(p).join("Library/Application Support/AudioMatrix"))
                .map_err(|_| PersistenceError::ConfigDirNotFound)
        }
        #[cfg(target_os = "linux")]
        {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|_| std::env::var("HOME").map(|p| PathBuf::from(p).join(".config")))
                .map(|p| p.join("audiomatrix"))
                .map_err(|_| PersistenceError::ConfigDirNotFound)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err(PersistenceError::ConfigDirNotFound)
        }
    }

    /// Loads configuration from a file.
    fn load_from_file(path: &Path) -> Result<PersistedConfig> {
        let content = fs::read_to_string(path)?;
        let config: PersistedConfig = serde_json::from_str(&content)?;
        info!("Loaded configuration from {}", path.display());
        Ok(config)
    }

    /// Sets whether auto-save is enabled.
    pub fn set_auto_save(&mut self, enabled: bool) {
        self.auto_save = enabled;
    }

    /// Returns the current configuration.
    #[must_use]
    pub fn config(&self) -> PersistedConfig {
        self.config.read().clone()
    }

    /// Updates the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn update<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut PersistedConfig),
    {
        {
            let mut config = self.config.write();
            f(&mut config);
        }
        if self.auto_save {
            self.save()?;
        }
        Ok(())
    }

    /// Saves the configuration to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> Result<()> {
        let config = self.config.read();
        let content = serde_json::to_string_pretty(&*config)?;
        drop(config);

        // Write to temp file first, then rename (atomic on most systems)
        let temp_path = self.path.with_extension("json.tmp");
        fs::write(&temp_path, &content)?;
        fs::rename(&temp_path, &self.path)?;

        debug!("Saved configuration to {}", self.path.display());
        Ok(())
    }

    /// Reloads the configuration from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn reload(&self) -> Result<()> {
        if self.path.exists() {
            let new_config = Self::load_from_file(&self.path)?;
            *self.config.write() = new_config;
        } else {
            warn!("Configuration file does not exist: {}", self.path.display());
        }
        Ok(())
    }

    /// Returns the configuration file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Adds a route to the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn add_route(&self, route: PersistedRoute) -> Result<()> {
        self.update(|config| {
            config.add_route(route);
        })
    }

    /// Removes a route from the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    #[allow(clippy::too_many_arguments)]
    pub fn remove_route(
        &self,
        source_node: &str,
        source_device: &str,
        source_channel: u16,
        dest_node: &str,
        dest_device: &str,
        dest_channel: u16,
    ) -> Result<bool> {
        let mut removed = false;
        self.update(|config| {
            removed = config.remove_route(
                source_node,
                source_device,
                source_channel,
                dest_node,
                dest_device,
                dest_channel,
            );
        })?;
        Ok(removed)
    }

    /// Returns all routes.
    #[must_use]
    pub fn routes(&self) -> Vec<PersistedRoute> {
        self.config.read().routes.clone()
    }

    /// Records a device as attached.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn attach_device(&self, device_id: String, display_name: Option<String>) -> Result<()> {
        self.update(|config| {
            config.attach_device(device_id, display_name);
        })
    }

    /// Records a device as detached.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn detach_device(&self, device_id: &str) -> Result<()> {
        self.update(|config| {
            config.detach_device(device_id);
        })
    }

    /// Returns the list of device IDs that were attached.
    #[must_use]
    pub fn attached_device_ids(&self) -> Vec<String> {
        self.config
            .read()
            .attached_device_ids()
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// Returns attached device info (ID and display name).
    #[must_use]
    pub fn attached_devices(&self) -> Vec<PersistedDevice> {
        self.config
            .read()
            .attached_devices
            .iter()
            .filter(|d| d.attached)
            .cloned()
            .collect()
    }

    /// Sets a channel label.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn set_channel_label(&self, device_id: &str, channel: u16, label: String) -> Result<bool> {
        let mut success = false;
        self.update(|config| {
            success = config.set_channel_label(device_id, channel, label);
        })?;
        Ok(success)
    }

    /// Gets channel labels for a device.
    #[must_use]
    pub fn get_channel_labels(&self, device_id: &str) -> Option<ChannelLabels> {
        self.config.read().get_channel_labels(device_id).cloned()
    }

    /// Adds a virtual device configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn add_virtual_device(&self, config: VirtualDeviceConfig) -> Result<()> {
        self.update(|c| {
            c.add_virtual_device(config);
        })
    }

    /// Removes a virtual device configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if auto-save is enabled and saving fails.
    pub fn remove_virtual_device(&self, name: &str) -> Result<bool> {
        let mut removed = false;
        self.update(|config| {
            removed = config.remove_virtual_device(name);
        })?;
        Ok(removed)
    }

    /// Returns all virtual device configurations.
    #[must_use]
    pub fn virtual_devices(&self) -> Vec<VirtualDeviceConfig> {
        self.config.read().virtual_devices.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn config_store_new_file() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp); // Delete the file

        let store = ConfigStore::new(&path).unwrap();
        assert!(store.config().routes.is_empty());

        // File shouldn't exist yet until save
        assert!(!path.exists());
    }

    #[test]
    fn config_store_load_existing() {
        let mut temp = NamedTempFile::new().unwrap();
        let config = PersistedConfig::with_node("Loaded".into(), 1234, 5678);
        let json = serde_json::to_string(&config).unwrap();
        temp.write_all(json.as_bytes()).unwrap();
        temp.flush().unwrap();

        let store = ConfigStore::new(temp.path()).unwrap();
        let loaded = store.config();
        let node = loaded.node.unwrap();
        assert_eq!(node.name, "Loaded");
        assert_eq!(node.api_port, 1234);
    }

    #[test]
    fn config_store_save_and_reload() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp); // Delete the empty file

        let store = ConfigStore::new(&path).unwrap();
        store
            .update(|c| {
                c.set_setting("key".into(), "value".into());
            })
            .unwrap();

        // Verify file was written
        assert!(path.exists());

        // Reload and verify
        store.reload().unwrap();
        assert_eq!(
            store.config().get_setting("key"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn config_store_add_remove_routes() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp); // Delete the empty file

        let mut store = ConfigStore::new(&path).unwrap();
        store.set_auto_save(false); // Disable for faster test

        let route = PersistedRoute {
            source_node: "a".into(),
            source_device: "d1".into(),
            source_channel: 1,
            destination_node: "b".into(),
            destination_device: "d2".into(),
            destination_channel: 2,
            volume: 1.0,
            muted: false,
        };

        store.add_route(route).unwrap();
        assert_eq!(store.routes().len(), 1);

        let removed = store.remove_route("a", "d1", 1, "b", "d2", 2).unwrap();
        assert!(removed);
        assert!(store.routes().is_empty());
    }

    #[test]
    fn config_store_device_attachment() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp);

        let mut store = ConfigStore::new(&path).unwrap();
        store.set_auto_save(false);

        store
            .attach_device("dev-1".into(), Some("Mic".into()))
            .unwrap();
        store.attach_device("dev-2".into(), None).unwrap();

        let attached = store.attached_device_ids();
        assert_eq!(attached.len(), 2);

        store.detach_device("dev-1").unwrap();
        assert_eq!(store.attached_device_ids().len(), 1);
    }

    #[test]
    fn config_store_channel_labels() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp);

        let mut store = ConfigStore::new(&path).unwrap();
        store.set_auto_save(false);

        store.set_channel_label("dev-1", 1, "Kick".into()).unwrap();

        let labels = store.get_channel_labels("dev-1").unwrap();
        assert_eq!(labels.get_label(1), Some(&"Kick".to_string()));
    }

    #[test]
    fn config_store_virtual_devices() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        drop(temp);

        let mut store = ConfigStore::new(&path).unwrap();
        store.set_auto_save(false);

        store
            .add_virtual_device(VirtualDeviceConfig {
                name: "Test Virtual".into(),
                input_channels: 8,
                output_channels: 8,
                sample_rate: 96000,
                buffer_size: 128,
            })
            .unwrap();

        assert_eq!(store.virtual_devices().len(), 1);

        store.remove_virtual_device("Test Virtual").unwrap();
        assert!(store.virtual_devices().is_empty());
    }
}
