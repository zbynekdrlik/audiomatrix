//! Error types for ram-discovery.

use thiserror::Error;

/// Result type for ram-discovery operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ram-discovery.
#[derive(Debug, Error)]
pub enum Error {
    /// mDNS daemon error.
    #[error("mDNS error: {0}")]
    Mdns(String),

    /// Service registration failed.
    #[error("failed to register service: {0}")]
    Registration(String),

    /// Service not found.
    #[error("service not found: {0}")]
    NotFound(String),
}
