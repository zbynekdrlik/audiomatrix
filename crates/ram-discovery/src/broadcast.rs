//! UDP Broadcast Discovery
//!
//! A simple, reliable discovery mechanism using UDP broadcast.
//! This serves as a fallback when mDNS doesn't work reliably.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};

use crate::Result;

/// Default port for broadcast discovery.
pub const BROADCAST_PORT: u16 = 6981;

/// Magic bytes to identify AudioMatrix broadcast packets.
const MAGIC: &[u8; 4] = b"AMBC";

/// Protocol version.
const PROTOCOL_VERSION: u8 = 1;

/// Broadcast announcement message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastAnnouncement {
    /// Node name.
    pub name: String,
    /// API port.
    pub api_port: u16,
    /// VBAN port.
    pub vban_port: u16,
    /// Number of input channels.
    pub input_channels: u16,
    /// Number of output channels.
    pub output_channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Version string.
    pub version: String,
}

impl BroadcastAnnouncement {
    /// Serializes to wire format.
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(MAGIC);
        buf.push(PROTOCOL_VERSION);

        // Serialize using JSON for simplicity (could optimize later)
        let json = serde_json::to_vec(self).unwrap_or_default();
        let len = (json.len() as u16).to_le_bytes();
        buf.extend_from_slice(&len);
        buf.extend_from_slice(&json);

        buf
    }

    /// Deserializes from wire format.
    fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 7 {
            return None;
        }

        // Check magic
        if &data[0..4] != MAGIC {
            return None;
        }

        // Check version
        if data[4] != PROTOCOL_VERSION {
            return None;
        }

        // Get length
        let len = u16::from_le_bytes([data[5], data[6]]) as usize;
        if data.len() < 7 + len {
            return None;
        }

        // Parse JSON
        serde_json::from_slice(&data[7..7 + len]).ok()
    }
}

/// Information about a discovered node.
#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    /// Node name.
    pub name: String,
    /// IP address.
    pub address: IpAddr,
    /// API port.
    pub api_port: u16,
    /// VBAN port.
    pub vban_port: u16,
    /// Input channels.
    pub input_channels: u16,
    /// Output channels.
    pub output_channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Version.
    pub version: String,
    /// Last seen timestamp.
    pub last_seen: Instant,
}

/// Event from broadcast discovery.
#[derive(Debug, Clone)]
pub enum BroadcastEvent {
    /// Node discovered or updated.
    NodeDiscovered(DiscoveredNode),
    /// Node went offline (timeout).
    NodeTimeout(String),
}

/// Configuration for broadcast discovery.
#[derive(Debug, Clone)]
pub struct BroadcastConfig {
    /// Port to use for discovery.
    pub port: u16,
    /// Announcement interval.
    pub announce_interval: Duration,
    /// Node timeout (how long before marking offline).
    pub node_timeout: Duration,
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self {
            port: BROADCAST_PORT,
            announce_interval: Duration::from_secs(5),
            node_timeout: Duration::from_secs(15),
        }
    }
}

/// UDP Broadcast discovery service.
pub struct BroadcastDiscovery {
    config: BroadcastConfig,
    announcement: Arc<RwLock<BroadcastAnnouncement>>,
    nodes: Arc<RwLock<HashMap<String, DiscoveredNode>>>,
    running: Arc<AtomicBool>,
    event_senders: Arc<RwLock<Vec<crossbeam_channel::Sender<BroadcastEvent>>>>,
}

impl BroadcastDiscovery {
    /// Creates a new broadcast discovery service.
    ///
    /// # Errors
    ///
    /// Returns an error if socket binding fails.
    pub fn new(announcement: BroadcastAnnouncement, config: BroadcastConfig) -> Result<Self> {
        Ok(Self {
            config,
            announcement: Arc::new(RwLock::new(announcement)),
            nodes: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            event_senders: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Creates with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if socket binding fails.
    pub fn with_defaults(announcement: BroadcastAnnouncement) -> Result<Self> {
        Self::new(announcement, BroadcastConfig::default())
    }

    /// Subscribes to discovery events.
    #[must_use]
    pub fn subscribe(&self) -> crossbeam_channel::Receiver<BroadcastEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.event_senders.write().push(tx);
        rx
    }

    /// Starts the discovery service.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound.
    pub fn start(&self) -> Result<()> {
        if self.running.load(Ordering::Acquire) {
            return Ok(());
        }

        self.running.store(true, Ordering::Release);

        // Clone what we need for threads
        let running = self.running.clone();
        let announcement = self.announcement.clone();
        let config = self.config.clone();

        // Start sender thread
        std::thread::spawn(move || {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind broadcast sender socket: {e}");
                    return;
                },
            };

            if let Err(e) = socket.set_broadcast(true) {
                warn!("Failed to enable broadcast: {e}");
                return;
            }

            let broadcast_addr =
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), config.port);

            info!("Broadcast discovery sender started on port {}", config.port);

            while running.load(Ordering::Acquire) {
                let data = announcement.read().to_bytes();
                if let Err(e) = socket.send_to(&data, broadcast_addr) {
                    debug!("Broadcast send error: {e}");
                }
                trace!("Sent broadcast announcement");

                std::thread::sleep(config.announce_interval);
            }

            debug!("Broadcast sender stopped");
        });

        // Clone for receiver thread
        let running = self.running.clone();
        let nodes = self.nodes.clone();
        let event_senders = self.event_senders.clone();
        let config = self.config.clone();
        let my_name = self.announcement.read().name.clone();

        // Start receiver thread
        std::thread::spawn(move || {
            let socket = match UdpSocket::bind(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                config.port,
            )) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind broadcast receiver socket: {e}");
                    return;
                },
            };

            if let Err(e) = socket.set_read_timeout(Some(Duration::from_millis(500))) {
                warn!("Failed to set socket timeout: {e}");
            }

            info!(
                "Broadcast discovery receiver listening on port {}",
                config.port
            );

            let mut buf = [0u8; 1024];

            while running.load(Ordering::Acquire) {
                match socket.recv_from(&mut buf) {
                    Ok((len, addr)) => {
                        if let Some(announcement) = BroadcastAnnouncement::from_bytes(&buf[..len]) {
                            // Skip our own announcements
                            if announcement.name == my_name {
                                continue;
                            }

                            let node = DiscoveredNode {
                                name: announcement.name.clone(),
                                address: addr.ip(),
                                api_port: announcement.api_port,
                                vban_port: announcement.vban_port,
                                input_channels: announcement.input_channels,
                                output_channels: announcement.output_channels,
                                sample_rate: announcement.sample_rate,
                                version: announcement.version,
                                last_seen: Instant::now(),
                            };

                            let is_new = !nodes.read().contains_key(&node.name);

                            nodes.write().insert(node.name.clone(), node.clone());

                            if is_new {
                                info!(
                                    "Discovered node via broadcast: {} at {}",
                                    node.name, node.address
                                );
                            }

                            // Emit event
                            let event = BroadcastEvent::NodeDiscovered(node);
                            let senders = event_senders.read();
                            for sender in senders.iter() {
                                let _ = sender.send(event.clone());
                            }
                        }
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Timeout, check for stale nodes
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        // Timeout, check for stale nodes
                    },
                    Err(e) => {
                        debug!("Broadcast recv error: {e}");
                    },
                }

                // Clean up stale nodes
                let now = Instant::now();
                let mut timed_out = Vec::new();

                {
                    let nodes_guard = nodes.read();
                    for (name, node) in nodes_guard.iter() {
                        if now.duration_since(node.last_seen) > config.node_timeout {
                            timed_out.push(name.clone());
                        }
                    }
                }

                for name in timed_out {
                    nodes.write().remove(&name);
                    info!("Node timed out: {name}");

                    let event = BroadcastEvent::NodeTimeout(name);
                    let senders = event_senders.read();
                    for sender in senders.iter() {
                        let _ = sender.send(event.clone());
                    }
                }
            }

            debug!("Broadcast receiver stopped");
        });

        info!("Broadcast discovery started");
        Ok(())
    }

    /// Returns all discovered nodes.
    #[must_use]
    pub fn nodes(&self) -> Vec<DiscoveredNode> {
        self.nodes.read().values().cloned().collect()
    }

    /// Returns a specific node by name.
    #[must_use]
    pub fn node(&self, name: &str) -> Option<DiscoveredNode> {
        self.nodes.read().get(name).cloned()
    }

    /// Returns the number of discovered nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.read().len()
    }

    /// Updates the announcement.
    pub fn update_announcement(&self, announcement: BroadcastAnnouncement) {
        *self.announcement.write() = announcement;
    }

    /// Stops the discovery service.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        info!("Broadcast discovery stopped");
    }

    /// Returns true if the service is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_roundtrip() {
        let ann = BroadcastAnnouncement {
            name: "test-node".to_string(),
            api_port: 8080,
            vban_port: 6980,
            input_channels: 8,
            output_channels: 8,
            sample_rate: 48000,
            version: "0.1.0".to_string(),
        };

        let bytes = ann.to_bytes();
        let parsed = BroadcastAnnouncement::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.name, "test-node");
        assert_eq!(parsed.api_port, 8080);
        assert_eq!(parsed.input_channels, 8);
    }

    #[test]
    fn invalid_magic_rejected() {
        let mut bytes = vec![0, 1, 2, 3]; // Wrong magic
        bytes.extend_from_slice(&[1, 0, 0]);

        assert!(BroadcastAnnouncement::from_bytes(&bytes).is_none());
    }

    #[test]
    fn config_default() {
        let config = BroadcastConfig::default();
        assert_eq!(config.port, BROADCAST_PORT);
        assert_eq!(config.announce_interval, Duration::from_secs(5));
    }
}
