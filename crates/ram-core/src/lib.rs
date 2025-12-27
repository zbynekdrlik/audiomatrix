//! RAM Core - Audio routing engine for `AudioMatrix`
//!
//! This crate provides the core audio processing functionality:
//! - Lock-free audio routing matrix
//! - Sample rate conversion
//! - Volume control and mixing
//! - Ring buffers for inter-thread communication

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod buffer;
pub mod error;
pub mod mixer;
pub mod routing;

pub use error::{Error, Result};

/// Audio sample type used throughout the system.
pub type Sample = f32;

/// Maximum number of channels supported per stream.
pub const MAX_CHANNELS: usize = 256;

/// Default buffer size in samples.
pub const DEFAULT_BUFFER_SIZE: usize = 256;
