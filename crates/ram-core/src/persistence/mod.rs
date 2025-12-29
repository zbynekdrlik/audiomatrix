//! Configuration persistence for `AudioMatrix`.
//!
//! This module provides functionality to save and load configuration
//! from disk, enabling state persistence across restarts.
//!
//! # Module Structure
//!
//! - `types`: Persisted data types (devices, routes, labels, etc.)
//! - `config`: The main `PersistedConfig` structure
//! - `store`: The `ConfigStore` for file-based persistence

mod config;
mod store;
mod types;

pub use config::PersistedConfig;
pub use store::ConfigStore;
pub use types::{
    ChannelLabels, PersistedDevice, PersistedNode, PersistedRoute, VirtualDeviceConfig,
};

use thiserror::Error;

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
