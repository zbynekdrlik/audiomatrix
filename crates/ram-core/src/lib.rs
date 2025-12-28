//! RAM Core - Audio routing engine for `AudioMatrix`
//!
//! This crate provides the core audio processing functionality:
//! - Lock-free audio routing matrix
//! - Sample rate conversion
//! - Volume control and mixing
//! - Ring buffers for inter-thread communication
//! - Audio device enumeration and management

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod active_stream;
pub mod atomic;
pub mod buffer;
pub mod callbacks;
pub mod connection;
pub mod destination;
pub mod device;
pub mod engine;
pub mod error;
pub mod latency;
pub mod metering;
pub mod mixer;
pub mod persistence;
pub mod resampler;
pub mod ring_buffer_pool;
pub mod route_controller;
pub mod routing;
pub mod routing_snapshot;
pub mod routing_table;
pub mod stream_registry;
pub mod subscription;
pub mod subscription_manager;

pub use atomic::AtomicF32;
pub use connection::{ConnectionId, ConnectionManager, ConnectionState, SourceConnection};
pub use destination::{DestinationChannel, HeadroomMode};
pub use device::{DeviceDirection, DeviceEvent, DeviceInfo, DeviceManager, DeviceState};
pub use engine::{AudioEngine, EngineConfig};
pub use persistence::{ConfigStore, PersistedConfig, PersistedRoute};
pub use resampler::{Resampler, ResamplerQuality};
pub use active_stream::{ActiveInputStream, ActiveOutputStream, StreamConfig, StreamState};
pub use ring_buffer_pool::RingBufferPool;
pub use routing_snapshot::{DestinationSnapshot, RoutingSnapshot, SourceSlot};
pub use routing_table::RoutingTable;
pub use stream_registry::StreamRegistry;
pub use subscription::{
    Subscription, SubscriptionConfig, SubscriptionId, SubscriptionMessage, SubscriptionState,
    SubscribeRequest, SubscribeAck, SubscribeResult, UnsubscribeRequest, UnsubscribeAck,
};
pub use subscription_manager::{SubscriptionManager, SubscriptionStats};
pub use latency::{CallbackTimer, LatencyCalculator, LatencyReport, TimingStats};
pub use metering::{ChannelMeter, MeterBank, MeterLevels};
pub use route_controller::{RouteController, RouteError, RouteResult};

pub use error::{Error, Result};

/// Audio sample type used throughout the system.
pub type Sample = f32;

/// Maximum number of channels supported per stream.
pub const MAX_CHANNELS: usize = 256;

/// Default buffer size in samples.
pub const DEFAULT_BUFFER_SIZE: usize = 256;
