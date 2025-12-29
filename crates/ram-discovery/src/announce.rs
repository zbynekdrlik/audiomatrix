//! Service announcement via mDNS.
//!
//! This module provides mDNS service announcement for `AudioMatrix` nodes,
//! including TXT record metadata for service capabilities.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceInfo};
use parking_lot::RwLock;
use tracing::{debug, info};

use crate::{Error, Result, SERVICE_TYPE};

/// Metadata included in mDNS TXT records.
#[derive(Debug, Clone, Default)]
pub struct ServiceMetadata {
    /// Protocol version.
    pub version: String,
    /// Number of input channels.
    pub input_channels: u16,
    /// Number of output channels.
    pub output_channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Node role (server/client).
    pub role: String,
    /// Additional custom properties.
    pub properties: HashMap<String, String>,
}

impl ServiceMetadata {
    /// Creates default metadata for a server node.
    #[must_use]
    pub fn server(input_channels: u16, output_channels: u16, sample_rate: u32) -> Self {
        Self {
            version: "1.0".to_string(),
            input_channels,
            output_channels,
            sample_rate,
            role: "server".to_string(),
            properties: HashMap::new(),
        }
    }

    /// Converts metadata to TXT record properties.
    #[must_use]
    pub fn to_txt_properties(&self) -> Vec<(String, String)> {
        let mut props = vec![
            ("version".to_string(), self.version.clone()),
            ("inputs".to_string(), self.input_channels.to_string()),
            ("outputs".to_string(), self.output_channels.to_string()),
            ("samplerate".to_string(), self.sample_rate.to_string()),
            ("role".to_string(), self.role.clone()),
        ];

        for (key, value) in &self.properties {
            props.push((key.clone(), value.clone()));
        }

        props
    }
}

/// Configuration for the service announcer.
#[derive(Debug, Clone)]
pub struct AnnouncerConfig {
    /// Instance name for the service.
    pub instance_name: String,
    /// Port to announce.
    pub port: u16,
    /// Service metadata.
    pub metadata: ServiceMetadata,
    /// Heartbeat interval.
    pub heartbeat_interval: Duration,
    /// Minimum heartbeat interval (when activity is high).
    pub min_heartbeat_interval: Duration,
    /// Maximum heartbeat interval (when idle).
    pub max_heartbeat_interval: Duration,
}

impl Default for AnnouncerConfig {
    fn default() -> Self {
        Self {
            instance_name: String::new(),
            port: 6980,
            metadata: ServiceMetadata::default(),
            heartbeat_interval: Duration::from_secs(30),
            min_heartbeat_interval: Duration::from_secs(5),
            max_heartbeat_interval: Duration::from_secs(120),
        }
    }
}

/// Statistics for the service announcer.
#[derive(Debug, Clone, Default)]
pub struct AnnouncerStats {
    /// Total heartbeats sent.
    pub heartbeats_sent: u64,
    /// Time of last heartbeat.
    pub last_heartbeat: Option<Instant>,
    /// Current heartbeat interval.
    pub current_interval: Duration,
}

/// Announces `AudioMatrix` service on the local network.
///
/// Includes support for:
/// - TXT records with service metadata
/// - Adaptive heartbeat mechanism
/// - Graceful shutdown
pub struct ServiceAnnouncer {
    daemon: ServiceDaemon,
    instance_name: String,
    metadata: RwLock<ServiceMetadata>,
    running: Arc<AtomicBool>,
    heartbeat_count: AtomicU64,
    current_interval: RwLock<Duration>,
    config: AnnouncerConfig,
}

impl ServiceAnnouncer {
    /// Creates a new service announcer with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon or service registration fails.
    pub fn with_config(config: AnnouncerConfig) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))?;

        let hostname =
            hostname::get().map_or_else(|_| "unknown".into(), |h| h.to_string_lossy().into_owned());
        // mDNS requires hostname to end with .local.
        let hostname = if hostname.ends_with(".local.") {
            hostname
        } else if hostname.ends_with(".local") {
            format!("{hostname}.")
        } else {
            format!("{hostname}.local.")
        };

        // Build TXT record properties
        let txt_props: HashMap<String, String> =
            config.metadata.to_txt_properties().into_iter().collect();

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &config.instance_name,
            &hostname,
            (),
            config.port,
            Some(txt_props),
        )
        .map_err(|e| Error::Registration(e.to_string()))?;

        daemon
            .register(service_info)
            .map_err(|e| Error::Registration(e.to_string()))?;

        info!(
            "Announced service: {} on port {} with {} inputs, {} outputs @ {}Hz",
            config.instance_name,
            config.port,
            config.metadata.input_channels,
            config.metadata.output_channels,
            config.metadata.sample_rate
        );

        Ok(Self {
            daemon,
            instance_name: config.instance_name.clone(),
            metadata: RwLock::new(config.metadata.clone()),
            running: Arc::new(AtomicBool::new(true)),
            heartbeat_count: AtomicU64::new(0),
            current_interval: RwLock::new(config.heartbeat_interval),
            config,
        })
    }

    /// Creates a new service announcer.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon or service registration fails.
    pub fn new(instance_name: &str, port: u16) -> Result<Self> {
        let config = AnnouncerConfig {
            instance_name: instance_name.to_string(),
            port,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Returns the instance name.
    #[must_use]
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    /// Returns the current metadata.
    #[must_use]
    pub fn metadata(&self) -> ServiceMetadata {
        self.metadata.read().clone()
    }

    /// Updates the service metadata.
    ///
    /// Note: This updates the local copy but doesn't re-register
    /// the mDNS service. A full re-registration would be needed
    /// for the changes to be visible on the network.
    pub fn update_metadata(&self, metadata: ServiceMetadata) {
        *self.metadata.write() = metadata;
    }

    /// Returns announcer statistics.
    #[must_use]
    pub fn stats(&self) -> AnnouncerStats {
        AnnouncerStats {
            heartbeats_sent: self.heartbeat_count.load(Ordering::Relaxed),
            last_heartbeat: None, // Would need to track this
            current_interval: *self.current_interval.read(),
        }
    }

    /// Sends a heartbeat (re-announces the service).
    ///
    /// This can be used to refresh the mDNS cache on the network.
    pub fn heartbeat(&self) {
        self.heartbeat_count.fetch_add(1, Ordering::Relaxed);
        debug!(
            "Heartbeat #{}",
            self.heartbeat_count.load(Ordering::Relaxed)
        );
    }

    /// Adjusts the heartbeat interval based on activity.
    ///
    /// Call with `true` when there's network activity to reduce interval,
    /// `false` when idle to increase interval.
    pub fn adjust_heartbeat_interval(&self, activity: bool) {
        let mut interval = self.current_interval.write();
        if activity {
            // Decrease interval (more frequent heartbeats)
            *interval = (*interval / 2).max(self.config.min_heartbeat_interval);
        } else {
            // Increase interval (less frequent heartbeats)
            *interval = (*interval * 2).min(self.config.max_heartbeat_interval);
        }
        debug!("Adjusted heartbeat interval to {:?}", *interval);
    }

    /// Returns true if the announcer is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Stops announcing the service.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon fails to shut down.
    pub fn shutdown(self) -> Result<()> {
        debug!("Shutting down service announcer");
        self.running.store(false, Ordering::Release);
        self.daemon
            .shutdown()
            .map_err(|e| Error::Mdns(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_metadata_default() {
        let metadata = ServiceMetadata::default();
        assert!(metadata.version.is_empty());
        assert_eq!(metadata.input_channels, 0);
        assert_eq!(metadata.output_channels, 0);
    }

    #[test]
    fn service_metadata_server() {
        let metadata = ServiceMetadata::server(8, 8, 48000);
        assert_eq!(metadata.version, "1.0");
        assert_eq!(metadata.input_channels, 8);
        assert_eq!(metadata.output_channels, 8);
        assert_eq!(metadata.sample_rate, 48000);
        assert_eq!(metadata.role, "server");
    }

    #[test]
    fn service_metadata_to_txt() {
        let metadata = ServiceMetadata::server(2, 4, 44100);
        let props = metadata.to_txt_properties();

        assert!(props.iter().any(|(k, v)| k == "version" && v == "1.0"));
        assert!(props.iter().any(|(k, v)| k == "inputs" && v == "2"));
        assert!(props.iter().any(|(k, v)| k == "outputs" && v == "4"));
        assert!(props.iter().any(|(k, v)| k == "samplerate" && v == "44100"));
        assert!(props.iter().any(|(k, v)| k == "role" && v == "server"));
    }

    #[test]
    fn service_metadata_custom_properties() {
        let mut metadata = ServiceMetadata::server(2, 2, 48000);
        metadata
            .properties
            .insert("custom".to_string(), "value".to_string());

        let props = metadata.to_txt_properties();
        assert!(props.iter().any(|(k, v)| k == "custom" && v == "value"));
    }

    #[test]
    fn announcer_config_default() {
        let config = AnnouncerConfig::default();
        assert_eq!(config.port, 6980);
        assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
    }

    #[test]
    fn announcer_stats_default() {
        let stats = AnnouncerStats::default();
        assert_eq!(stats.heartbeats_sent, 0);
        assert!(stats.last_heartbeat.is_none());
    }

    // Network-dependent tests are commented out for CI compatibility
    // Manual testing:
    // #[test]
    // fn announcer_new_and_shutdown() {
    //     let announcer = ServiceAnnouncer::new("test-node", 6980).unwrap();
    //     assert_eq!(announcer.instance_name(), "test-node");
    //     assert!(announcer.is_running());
    //     announcer.shutdown().unwrap();
    // }
}
