//! Error types for ram-core.

use thiserror::Error;

/// Result type for ram-core operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ram-core.
#[derive(Debug, Error)]
pub enum Error {
    /// Buffer capacity exceeded.
    #[error("buffer overflow: tried to write {attempted} samples but only {available} available")]
    BufferOverflow {
        /// Number of samples attempted to write.
        attempted: usize,
        /// Number of samples available in buffer.
        available: usize,
    },

    /// Invalid channel configuration.
    #[error("invalid channel count: {0} (max: {max})", max = crate::MAX_CHANNELS)]
    InvalidChannelCount(usize),

    /// Sample rate mismatch.
    #[error("sample rate mismatch: expected {expected} Hz, got {actual} Hz")]
    SampleRateMismatch {
        /// Expected sample rate.
        expected: u32,
        /// Actual sample rate.
        actual: u32,
    },

    /// Route not found.
    #[error("route not found: {0}")]
    RouteNotFound(String),
}
