//! RAM Core - Audio routing engine for `AudioMatrix`
//!
//! This crate provides the core audio processing functionality:
//! - Lock-free audio routing matrix
//! - Sample rate conversion
//! - Volume control and mixing
//! - Ring buffers for inter-thread communication

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod atomic;
pub mod buffer;
pub mod connection;
pub mod destination;
pub mod engine;
pub mod error;
pub mod mixer;
pub mod resampler;
pub mod routing;

pub use atomic::AtomicF32;
pub use connection::{ConnectionId, SourceConnection};
pub use destination::{DestinationChannel, HeadroomMode};
pub use engine::{AudioEngine, EngineConfig};
pub use resampler::{Resampler, ResamplerQuality};

pub use error::{Error, Result};

/// Audio sample type used throughout the system.
pub type Sample = f32;

/// Maximum number of channels supported per stream.
pub const MAX_CHANNELS: usize = 256;

/// Default buffer size in samples.
pub const DEFAULT_BUFFER_SIZE: usize = 256;
