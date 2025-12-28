//! RAM VBAN - VBAN protocol implementation for `AudioMatrix`
//!
//! This crate implements the VB-Audio Network (VBAN) protocol for
//! streaming audio over IP networks with low latency.
//!
//! # Protocol Overview
//!
//! VBAN uses UDP for transport with a 28-byte header followed by audio data.
//! The protocol supports multiple sample rates, formats, and channel counts.
//!
//! # Features
//!
//! - Lock-free packet pool for allocation-free transmission
//! - Adaptive jitter buffer for smooth reception
//! - Sequence tracking and packet reordering
//! - Stream lifecycle management with timeout detection

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod jitter;
pub mod pool;
pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod stream;

pub use error::{Error, Result};
pub use jitter::{JitterBuffer, JitterBufferConfig, JitterBufferState, JitterStats};
pub use pool::{PacketBuffer, PacketPool, PooledPacket};
pub use protocol::{VbanHeader, VbanProtocol, VbanSampleRate, VbanSubProtocol};
pub use receiver::VbanReceiver;
pub use sender::{VbanSender, VbanSenderConfig};
pub use stream::{ManagedReceiveStream, StreamEvent, StreamManager, StreamState};

/// Default VBAN port.
pub const DEFAULT_PORT: u16 = 6980;

/// Maximum VBAN packet size (header + audio data).
pub const MAX_PACKET_SIZE: usize = 1436;

/// VBAN header size in bytes.
pub const HEADER_SIZE: usize = 28;

/// Maximum audio data per packet.
pub const MAX_AUDIO_SIZE: usize = MAX_PACKET_SIZE - HEADER_SIZE;
