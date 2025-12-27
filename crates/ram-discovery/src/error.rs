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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_mdns() {
        let err = Error::Mdns("daemon failed".to_string());
        assert_eq!(err.to_string(), "mDNS error: daemon failed");
    }

    #[test]
    fn error_display_registration() {
        let err = Error::Registration("port in use".to_string());
        assert_eq!(err.to_string(), "failed to register service: port in use");
    }

    #[test]
    fn error_display_not_found() {
        let err = Error::NotFound("my-device".to_string());
        assert_eq!(err.to_string(), "service not found: my-device");
    }

    #[test]
    fn error_debug_impl() {
        let err = Error::Mdns("test".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("Mdns"));
    }
}
