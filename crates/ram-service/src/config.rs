//! Service configuration.
//!
//! Handles loading and managing configuration from files and environment.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Node name (defaults to hostname).
    #[serde(default = "default_node_name")]
    pub node_name: String,
    /// API server configuration.
    #[serde(default)]
    pub api: ApiConfig,
    /// VBAN configuration.
    #[serde(default)]
    pub vban: VbanConfig,
    /// Discovery configuration.
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Audio configuration.
    #[serde(default)]
    pub audio: AudioConfig,
}

fn default_node_name() -> String {
    hostname::get().map_or_else(
        |_| "AudioMatrix".into(),
        |h| h.to_string_lossy().into_owned(),
    )
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            node_name: default_node_name(),
            api: ApiConfig::default(),
            vban: VbanConfig::default(),
            discovery: DiscoveryConfig::default(),
            logging: LoggingConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

#[allow(dead_code)]
impl ServiceConfig {
    /// Loads configuration from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Saves configuration to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Creates a configuration with a specific node name.
    #[must_use]
    pub fn with_node_name(mut self, name: impl Into<String>) -> Self {
        self.node_name = name.into();
        self
    }

    /// Creates a configuration with a specific API port.
    #[must_use]
    pub fn with_api_port(mut self, port: u16) -> Self {
        self.api.port = port;
        self
    }

    /// Creates a configuration with a specific VBAN port.
    #[must_use]
    pub fn with_vban_port(mut self, port: u16) -> Self {
        self.vban.port = port;
        self
    }
}

/// API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Bind address.
    #[serde(default = "default_bind_addr")]
    pub bind: IpAddr,
    /// Port number.
    #[serde(default = "default_api_port")]
    pub port: u16,
}

fn default_bind_addr() -> IpAddr {
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}

fn default_api_port() -> u16 {
    8080
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_bind_addr(),
            port: default_api_port(),
        }
    }
}

/// VBAN configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VbanConfig {
    /// Whether VBAN is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Bind address.
    #[serde(default = "default_bind_addr")]
    pub bind: IpAddr,
    /// Port number.
    #[serde(default = "default_vban_port")]
    pub port: u16,
}

fn default_true() -> bool {
    true
}

fn default_vban_port() -> u16 {
    6980
}

impl Default for VbanConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: default_bind_addr(),
            port: default_vban_port(),
        }
    }
}

/// Service discovery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Whether discovery is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Service type for mDNS.
    #[serde(default = "default_service_type")]
    pub service_type: String,
}

fn default_service_type() -> String {
    "_audiomatrix._tcp.local.".to_string()
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_type: default_service_type(),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level.
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Include target in logs.
    #[serde(default)]
    pub include_target: bool,
    /// Include file location in logs.
    #[serde(default)]
    pub include_location: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            include_target: false,
            include_location: false,
        }
    }
}

/// Audio configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Whether to auto-start default audio devices on startup.
    /// CRITICAL: Default is FALSE for zero auto-connect policy.
    /// Professional audio engineers need explicit control over device attachment.
    #[serde(default)]
    pub auto_start_devices: bool,

    /// Default sample rate for new virtual devices.
    /// Only 96000, 48000, 44100 Hz are supported.
    #[serde(default = "default_sample_rate")]
    pub default_sample_rate: u32,

    /// Default buffer size in samples.
    /// Must be power of 2: 32, 64, 128, 256, 512, 1024, 2048.
    #[serde(default = "default_buffer_size")]
    pub default_buffer_size: u32,
}

fn default_sample_rate() -> u32 {
    48000
}

fn default_buffer_size() -> u32 {
    64
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            auto_start_devices: false, // CRITICAL: Zero auto-connect by default
            default_sample_rate: default_sample_rate(),
            default_buffer_size: default_buffer_size(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_config_default() {
        let config = ServiceConfig::default();
        assert!(!config.node_name.is_empty());
        assert_eq!(config.api.port, 8080);
        assert_eq!(config.vban.port, 6980);
        assert!(config.discovery.enabled);
        // CRITICAL: Zero auto-connect by default
        assert!(!config.audio.auto_start_devices);
    }

    #[test]
    fn service_config_with_node_name() {
        let config = ServiceConfig::default().with_node_name("TestNode");
        assert_eq!(config.node_name, "TestNode");
    }

    #[test]
    fn service_config_with_ports() {
        let config = ServiceConfig::default()
            .with_api_port(9000)
            .with_vban_port(7000);
        assert_eq!(config.api.port, 9000);
        assert_eq!(config.vban.port, 7000);
    }

    #[test]
    fn api_config_default() {
        let config = ApiConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.bind, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }

    #[test]
    fn vban_config_default() {
        let config = VbanConfig::default();
        assert!(config.enabled);
        assert_eq!(config.port, 6980);
    }

    #[test]
    fn discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(config.enabled);
        assert!(config.service_type.contains("audiomatrix"));
    }

    #[test]
    fn logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
        assert!(!config.include_target);
    }

    #[test]
    fn audio_config_default() {
        let config = AudioConfig::default();
        // CRITICAL: Zero auto-connect policy - must be false by default
        assert!(!config.auto_start_devices);
        assert_eq!(config.default_sample_rate, 48000);
        assert_eq!(config.default_buffer_size, 64);
    }

    #[test]
    fn audio_config_sample_rates() {
        // Only 96000, 48000, 44100 are supported
        let config = AudioConfig::default();
        let supported = [96000, 48000, 44100];
        assert!(supported.contains(&config.default_sample_rate));
    }

    #[test]
    fn service_config_serialization() {
        let config = ServiceConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ServiceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node_name, config.node_name);
    }
}
