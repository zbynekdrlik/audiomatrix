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
// Allow common patterns in audio/real-time code
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::type_complexity)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::float_cmp)]
#![allow(clippy::missing_fields_in_debug)]

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
pub mod wave_generator;

pub use active_stream::{ActiveInputStream, ActiveOutputStream, StreamConfig, StreamState};
pub use atomic::AtomicF32;
pub use connection::{ConnectionId, ConnectionManager, ConnectionState, SourceConnection};
pub use destination::{DestinationChannel, HeadroomMode};
pub use device::{
    AttachmentState, DeviceDirection, DeviceEvent, DeviceInfo, DeviceManager, DeviceState,
};
pub use engine::{AudioEngine, EngineConfig};
pub use latency::{CallbackTimer, LatencyCalculator, LatencyReport, TimingStats};
pub use metering::{ChannelMeter, MeterBank, MeterLevels};
pub use persistence::{
    ChannelLabels, ConfigStore, PersistedConfig, PersistedDevice, PersistedNode, PersistedRoute,
    VirtualDeviceConfig,
};
pub use resampler::{Resampler, ResamplerQuality};
pub use ring_buffer_pool::RingBufferPool;
pub use route_controller::{RouteController, RouteError, RouteResult};
pub use routing_snapshot::{DestinationSnapshot, RoutingSnapshot, SourceSlot};
pub use routing_table::RoutingTable;
pub use stream_registry::StreamRegistry;
pub use subscription::{
    SubscribeAck, SubscribeRequest, SubscribeResult, Subscription, SubscriptionConfig,
    SubscriptionId, SubscriptionMessage, SubscriptionState, UnsubscribeAck, UnsubscribeRequest,
};
pub use subscription_manager::{SubscriptionManager, SubscriptionStats};
pub use wave_generator::{
    WaveGeneratorConfig, WaveGeneratorState, WaveformType, DEFAULT_LEVEL_DBFS, MAX_LEVEL_DBFS,
};

pub use error::{Error, Result};

/// Audio sample type used throughout the system.
pub type Sample = f32;

/// Maximum number of channels supported per stream.
pub const MAX_CHANNELS: usize = 256;

/// Default buffer size in samples.
pub const DEFAULT_BUFFER_SIZE: usize = 256;
