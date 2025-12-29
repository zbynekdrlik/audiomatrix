//! Device management for audio hardware enumeration and state tracking.
//!
//! This module provides:
//! - Cross-platform audio device enumeration via cpal
//! - Device state machine (starting, running, error, stopped)
//! - Hot-plug detection for device arrival/removal
//! - Device cache with TTL for efficient polling

mod manager;
mod types;

pub use manager::{DeviceManager, DeviceManagerConfig};
pub use types::{
    AtomicAttachmentState, AtomicDeviceState, AttachmentState, DeviceConfig, DeviceDirection,
    DeviceEvent, DeviceInfo, DeviceState, SampleFormat,
};
