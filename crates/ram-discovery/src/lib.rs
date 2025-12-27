//! RAM Discovery - Service discovery for `AudioMatrix`
//!
//! This crate provides mDNS/DNS-SD based service discovery for finding
//! `AudioMatrix` nodes on the local network.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod announce;
pub mod browse;
pub mod error;

pub use announce::ServiceAnnouncer;
pub use browse::ServiceBrowser;
pub use error::{Error, Result};

/// Service type for `AudioMatrix` nodes.
pub const SERVICE_TYPE: &str = "_audiomatrix._tcp.local.";

/// Service type for VBAN streams (for compatibility).
pub const VBAN_SERVICE_TYPE: &str = "_vban._udp.local.";

/// Default mDNS port.
pub const MDNS_PORT: u16 = 5353;
