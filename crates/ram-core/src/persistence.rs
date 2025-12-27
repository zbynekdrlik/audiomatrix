//! Configuration persistence for `AudioMatrix`.
//!
//! This module provides functionality to save and load configuration
//! from disk, enabling state persistence across restarts.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};

/// Persistence errors.
#[derive(Debug, Error)]
pub enum PersistenceError {
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    /// Configuration directory not found.
    #[error("Configuration directory not found")]
    ConfigDirNotFound,
}

/// Result type for persistence operations.
pub type Result<T> = std::result::Result<T, PersistenceError>;

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
    /// Custom key-value settings.
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

fn default_version() -> u32 {
    1
}

impl PersistedConfig {
    /// Creates a new empty configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            node: None,
            routes: Vec::new(),
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
            settings: HashMap::new(),
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
}
