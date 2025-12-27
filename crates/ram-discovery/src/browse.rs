//! Service browsing via mDNS.
//!
//! This module provides mDNS service discovery for `AudioMatrix` nodes,
//! with support for filtering, event subscriptions, and metadata parsing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{Error, Result, SERVICE_TYPE};

/// Information about a discovered `AudioMatrix` node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node name (instance name).
    pub name: String,
    /// Full service name.
    pub fullname: String,
    /// Hostname.
    pub hostname: String,
    /// IP addresses.
    pub addresses: Vec<String>,
    /// API port.
    pub port: u16,
    /// Protocol version.
    pub version: Option<String>,
    /// Number of input channels.
    pub input_channels: Option<u16>,
    /// Number of output channels.
    pub output_channels: Option<u16>,
    /// Sample rate in Hz.
    pub sample_rate: Option<u32>,
    /// Node role.
    pub role: Option<String>,
    /// All TXT record properties.
    pub properties: HashMap<String, String>,
    /// When this node was first discovered.
    pub discovered_at: u64,
    /// When this node was last seen.
    pub last_seen: u64,
}

impl NodeInfo {
    /// Returns true if this node matches the given filter.
    #[must_use]
    pub fn matches_filter(&self, filter: &NodeFilter) -> bool {
        if let Some(min_inputs) = filter.min_input_channels {
            if self.input_channels.unwrap_or(0) < min_inputs {
                return false;
            }
        }
        if let Some(min_outputs) = filter.min_output_channels {
            if self.output_channels.unwrap_or(0) < min_outputs {
                return false;
            }
        }
        if let Some(ref role) = filter.role {
            if self.role.as_ref() != Some(role) {
                return false;
            }
        }
        if let Some(ref name_contains) = filter.name_contains {
            if !self
                .name
                .to_lowercase()
                .contains(&name_contains.to_lowercase())
            {
                return false;
            }
        }
        true
    }
}

/// Filter criteria for node discovery.
#[derive(Debug, Clone, Default)]
pub struct NodeFilter {
    /// Minimum number of input channels required.
    pub min_input_channels: Option<u16>,
    /// Minimum number of output channels required.
    pub min_output_channels: Option<u16>,
    /// Required role (server/client).
    pub role: Option<String>,
    /// Name must contain this string (case-insensitive).
    pub name_contains: Option<String>,
}

impl NodeFilter {
    /// Creates a filter for server nodes only.
    #[must_use]
    pub fn servers_only() -> Self {
        Self {
            role: Some("server".to_string()),
            ..Default::default()
        }
    }

    /// Creates a filter requiring minimum channel counts.
    #[must_use]
    pub fn with_channels(min_inputs: u16, min_outputs: u16) -> Self {
        Self {
            min_input_channels: Some(min_inputs),
            min_output_channels: Some(min_outputs),
            ..Default::default()
        }
    }
}

/// Event emitted when node discovery state changes.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new node was discovered.
    NodeDiscovered(NodeInfo),
    /// A node was updated (e.g., address changed).
    NodeUpdated(NodeInfo),
    /// A node was removed (no longer responding).
    NodeRemoved { name: String },
    /// Discovery encountered an error.
    Error(String),
}

/// Configuration for the service browser.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Filter to apply to discovered nodes.
    pub filter: Option<NodeFilter>,
    /// Whether to automatically start browsing on creation.
    pub auto_start: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            filter: None,
            auto_start: true,
        }
    }
}

/// Browses for `AudioMatrix` services on the local network.
///
/// Supports:
/// - TXT record metadata parsing
/// - Node filtering
/// - Event subscriptions
pub struct ServiceBrowser {
    daemon: ServiceDaemon,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    filter: Option<NodeFilter>,
    running: Arc<AtomicBool>,
    event_senders: Arc<RwLock<Vec<crossbeam_channel::Sender<DiscoveryEvent>>>>,
}

impl ServiceBrowser {
    /// Creates a new service browser with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be created.
    pub fn with_config(config: BrowserConfig) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| Error::Mdns(e.to_string()))?;

        let browser = Self {
            daemon,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            filter: config.filter,
            running: Arc::new(AtomicBool::new(false)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
        };

        if config.auto_start {
            browser.start()?;
        }

        Ok(browser)
    }

    /// Creates a new service browser.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be created.
    pub fn new() -> Result<Self> {
        Self::with_config(BrowserConfig::default())
    }

    /// Subscribes to discovery events.
    ///
    /// Returns a receiver that will receive discovery events.
    #[must_use]
    pub fn subscribe(&self) -> crossbeam_channel::Receiver<DiscoveryEvent> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        self.event_senders.write().push(sender);
        receiver
    }

    /// Starts browsing for services.
    ///
    /// # Errors
    ///
    /// Returns an error if browsing cannot be started.
    pub fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Acquire) {
            return Ok(()); // Already running
        }

        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| Error::Mdns(e.to_string()))?;

        self.running.store(true, Ordering::Release);

        let nodes = self.nodes.clone();
        let filter = self.filter.clone();
        let running = self.running.clone();
        let event_senders = self.event_senders.clone();

        std::thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(event) => {
                        Self::handle_event(event, &nodes, filter.as_ref(), &event_senders);
                    },
                    Err(flume::RecvTimeoutError::Timeout) => {},
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                }
            }
            debug!("Service browser thread exiting");
        });

        info!("Started browsing for AudioMatrix services");
        Ok(())
    }

    fn handle_event(
        event: ServiceEvent,
        nodes: &Arc<RwLock<HashMap<String, NodeInfo>>>,
        filter: Option<&NodeFilter>,
        event_senders: &Arc<RwLock<Vec<crossbeam_channel::Sender<DiscoveryEvent>>>>,
    ) {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // Parse TXT record properties
                let mut properties = HashMap::new();
                for prop in info.get_properties().iter() {
                    properties.insert(prop.key().to_string(), prop.val_str().to_string());
                }

                let node = NodeInfo {
                    name: info
                        .get_fullname()
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    fullname: info.get_fullname().to_string(),
                    hostname: info.get_hostname().to_string(),
                    addresses: info
                        .get_addresses()
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
                    port: info.get_port(),
                    version: properties.get("version").cloned(),
                    input_channels: properties.get("inputs").and_then(|v| v.parse().ok()),
                    output_channels: properties.get("outputs").and_then(|v| v.parse().ok()),
                    sample_rate: properties.get("samplerate").and_then(|v| v.parse().ok()),
                    role: properties.get("role").cloned(),
                    properties,
                    discovered_at: now,
                    last_seen: now,
                };

                // Apply filter if set
                if let Some(f) = filter {
                    if !node.matches_filter(f) {
                        debug!("Node {} filtered out", node.name);
                        return;
                    }
                }

                let is_update = nodes.read().contains_key(&node.fullname);
                info!("Discovered node: {} ({})", node.name, node.hostname);
                nodes.write().insert(node.fullname.clone(), node.clone());

                let event = if is_update {
                    DiscoveryEvent::NodeUpdated(node)
                } else {
                    DiscoveryEvent::NodeDiscovered(node)
                };

                Self::emit_event(event_senders, &event);
            },
            ServiceEvent::ServiceRemoved(_, fullname) => {
                debug!("Node removed: {fullname}");
                if nodes.write().remove(&fullname).is_some() {
                    Self::emit_event(
                        event_senders,
                        &DiscoveryEvent::NodeRemoved { name: fullname },
                    );
                }
            },
            ServiceEvent::SearchStarted(_) => {
                debug!("mDNS search started");
            },
            ServiceEvent::SearchStopped(_) => {
                debug!("mDNS search stopped");
            },
            ServiceEvent::ServiceFound(..) => {
                // ServiceFound is emitted before ServiceResolved, we wait for resolution
            },
        }
    }

    fn emit_event(
        senders: &Arc<RwLock<Vec<crossbeam_channel::Sender<DiscoveryEvent>>>>,
        event: &DiscoveryEvent,
    ) {
        let senders_guard = senders.read();
        let mut disconnected = Vec::new();

        for (i, sender) in senders_guard.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                disconnected.push(i);
            }
        }

        drop(senders_guard);

        if !disconnected.is_empty() {
            let mut senders_mut = senders.write();
            for i in disconnected.into_iter().rev() {
                senders_mut.remove(i);
            }
        }
    }

    /// Returns all discovered nodes.
    #[must_use]
    pub fn nodes(&self) -> Vec<NodeInfo> {
        self.nodes.read().values().cloned().collect()
    }

    /// Returns nodes matching the given filter.
    #[must_use]
    pub fn nodes_filtered(&self, filter: &NodeFilter) -> Vec<NodeInfo> {
        self.nodes
            .read()
            .values()
            .filter(|n| n.matches_filter(filter))
            .cloned()
            .collect()
    }

    /// Returns a specific node by name.
    #[must_use]
    pub fn node(&self, name: &str) -> Option<NodeInfo> {
        self.nodes.read().get(name).cloned()
    }

    /// Returns a node by partial name match.
    #[must_use]
    pub fn find_node(&self, name_contains: &str) -> Option<NodeInfo> {
        let lower = name_contains.to_lowercase();
        self.nodes
            .read()
            .values()
            .find(|n| n.name.to_lowercase().contains(&lower))
            .cloned()
    }

    /// Returns the number of discovered nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Returns true if the browser is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Stops browsing.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon fails to shut down.
    pub fn shutdown(self) -> Result<()> {
        self.running.store(false, Ordering::Release);
        self.daemon
            .shutdown()
            .map_err(|e| Error::Mdns(e.to_string()))?;
        info!("Service browser shut down");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_filter_default() {
        let filter = NodeFilter::default();
        assert!(filter.min_input_channels.is_none());
        assert!(filter.min_output_channels.is_none());
        assert!(filter.role.is_none());
    }

    #[test]
    fn node_filter_servers_only() {
        let filter = NodeFilter::servers_only();
        assert_eq!(filter.role, Some("server".to_string()));
    }

    #[test]
    fn node_filter_with_channels() {
        let filter = NodeFilter::with_channels(2, 4);
        assert_eq!(filter.min_input_channels, Some(2));
        assert_eq!(filter.min_output_channels, Some(4));
    }

    #[test]
    fn node_info_matches_filter_empty() {
        let node = NodeInfo {
            name: "test".to_string(),
            fullname: "test._audiomatrix._tcp.local.".to_string(),
            hostname: "test.local".to_string(),
            addresses: vec!["192.168.1.100".to_string()],
            port: 6980,
            version: Some("1.0".to_string()),
            input_channels: Some(8),
            output_channels: Some(8),
            sample_rate: Some(48000),
            role: Some("server".to_string()),
            properties: HashMap::new(),
            discovered_at: 0,
            last_seen: 0,
        };

        let filter = NodeFilter::default();
        assert!(node.matches_filter(&filter));
    }

    #[test]
    fn node_info_matches_filter_channels() {
        let node = NodeInfo {
            name: "test".to_string(),
            fullname: "test._audiomatrix._tcp.local.".to_string(),
            hostname: "test.local".to_string(),
            addresses: vec!["192.168.1.100".to_string()],
            port: 6980,
            version: None,
            input_channels: Some(4),
            output_channels: Some(4),
            sample_rate: None,
            role: None,
            properties: HashMap::new(),
            discovered_at: 0,
            last_seen: 0,
        };

        let filter = NodeFilter::with_channels(2, 2);
        assert!(node.matches_filter(&filter));

        let filter = NodeFilter::with_channels(8, 8);
        assert!(!node.matches_filter(&filter));
    }

    #[test]
    fn node_info_matches_filter_role() {
        let node = NodeInfo {
            name: "test".to_string(),
            fullname: "test._audiomatrix._tcp.local.".to_string(),
            hostname: "test.local".to_string(),
            addresses: vec![],
            port: 6980,
            version: None,
            input_channels: None,
            output_channels: None,
            sample_rate: None,
            role: Some("server".to_string()),
            properties: HashMap::new(),
            discovered_at: 0,
            last_seen: 0,
        };

        let filter = NodeFilter::servers_only();
        assert!(node.matches_filter(&filter));

        let filter = NodeFilter {
            role: Some("client".to_string()),
            ..Default::default()
        };
        assert!(!node.matches_filter(&filter));
    }

    #[test]
    fn node_info_matches_filter_name() {
        let node = NodeInfo {
            name: "Studio-Main".to_string(),
            fullname: "Studio-Main._audiomatrix._tcp.local.".to_string(),
            hostname: "studio.local".to_string(),
            addresses: vec![],
            port: 6980,
            version: None,
            input_channels: None,
            output_channels: None,
            sample_rate: None,
            role: None,
            properties: HashMap::new(),
            discovered_at: 0,
            last_seen: 0,
        };

        let filter = NodeFilter {
            name_contains: Some("studio".to_string()),
            ..Default::default()
        };
        assert!(node.matches_filter(&filter));

        let filter = NodeFilter {
            name_contains: Some("office".to_string()),
            ..Default::default()
        };
        assert!(!node.matches_filter(&filter));
    }

    #[test]
    fn browser_config_default() {
        let config = BrowserConfig::default();
        assert!(config.filter.is_none());
        assert!(config.auto_start);
    }

    // Network-dependent tests commented out for CI
    // #[test]
    // fn browser_lifecycle() {
    //     let browser = ServiceBrowser::new().unwrap();
    //     assert!(browser.is_running());
    //     assert_eq!(browser.node_count(), 0);
    //     browser.shutdown().unwrap();
    // }
}
