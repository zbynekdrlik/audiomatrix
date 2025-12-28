//! RAM Discovery - Service discovery for `AudioMatrix`
//!
//! This crate provides mDNS/DNS-SD based service discovery for finding
//! `AudioMatrix` nodes on the local network.
//!
//! # Features
//!
//! - Service announcement with TXT record metadata
//! - Service browsing with filtering
//! - Adaptive heartbeat mechanism
//! - Event-based discovery notifications
//!
//! # Example
//!
//! ```ignore
//! use ram_discovery::{ServiceAnnouncer, ServiceBrowser, ServiceMetadata};
//!
//! // Announce a service
//! let metadata = ServiceMetadata::server(8, 8, 48000);
//! let announcer = ServiceAnnouncer::with_config(AnnouncerConfig {
//!     instance_name: "my-node".to_string(),
//!     port: 6980,
//!     metadata,
//!     ..Default::default()
//! })?;
//!
//! // Browse for services
//! let browser = ServiceBrowser::new()?;
//! let nodes = browser.nodes();
//! ```

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod announce;
pub mod broadcast;
pub mod browse;
pub mod error;

pub use announce::{AnnouncerConfig, AnnouncerStats, ServiceAnnouncer, ServiceMetadata};
pub use broadcast::{BroadcastAnnouncement, BroadcastConfig, BroadcastDiscovery, BroadcastEvent, DiscoveredNode, BROADCAST_PORT};
pub use browse::{BrowserConfig, DiscoveryEvent, NodeFilter, NodeInfo, ServiceBrowser};
pub use error::{Error, Result};

/// Service type for `AudioMatrix` nodes.
pub const SERVICE_TYPE: &str = "_audiomatrix._tcp.local.";

/// Service type for VBAN streams (for compatibility).
pub const VBAN_SERVICE_TYPE: &str = "_vban._udp.local.";

/// Default mDNS port.
pub const MDNS_PORT: u16 = 5353;

/// Default `AudioMatrix` API port.
pub const DEFAULT_API_PORT: u16 = 6980;
