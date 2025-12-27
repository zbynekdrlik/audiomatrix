//! Error types for ram-vban.

use thiserror::Error;

/// Result type for ram-vban operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in ram-vban.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid VBAN header.
    #[error("invalid VBAN header: {0}")]
    InvalidHeader(String),

    /// Unsupported sample rate.
    #[error("unsupported sample rate: {0} Hz")]
    UnsupportedSampleRate(u32),

    /// Network I/O error.
    #[error("network error: {0}")]
    Network(#[from] std::io::Error),

    /// Stream not found.
    #[error("stream not found: {0}")]
    StreamNotFound(String),

    /// Buffer underrun during playback.
    #[error("buffer underrun: expected {expected} samples, got {available}")]
    BufferUnderrun {
        /// Expected number of samples.
        expected: usize,
        /// Available samples in buffer.
        available: usize,
    },
}
