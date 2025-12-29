//! Device types and enums.
//!
//! This module contains the core types used for device management:
//! - Direction and state enums
//! - Device configuration structures
//! - Device information

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

/// Device direction (input or output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceDirection {
    /// Audio input device (microphone, line in, etc.)
    Input,
    /// Audio output device (speakers, headphones, etc.)
    Output,
}

impl std::fmt::Display for DeviceDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input => write!(f, "input"),
            Self::Output => write!(f, "output"),
        }
    }
}

/// Current state of an audio device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DeviceState {
    /// Device is available but not in use.
    #[default]
    Idle = 0,
    /// Device is starting up.
    Starting = 1,
    /// Device is running and processing audio.
    Running = 2,
    /// Device encountered an error.
    Error = 3,
    /// Device has been stopped.
    Stopped = 4,
    /// Device has been disconnected.
    Disconnected = 5,
}

impl DeviceState {
    /// Converts from a u8 value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Starting),
            2 => Some(Self::Running),
            3 => Some(Self::Error),
            4 => Some(Self::Stopped),
            5 => Some(Self::Disconnected),
            _ => None,
        }
    }

    /// Returns true if the device is available for use.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Idle | Self::Stopped)
    }

    /// Returns true if the device is actively processing.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Error => write!(f, "error"),
            Self::Stopped => write!(f, "stopped"),
            Self::Disconnected => write!(f, "disconnected"),
        }
    }
}

/// User-controlled device attachment state.
///
/// This is separate from `DeviceState` which tracks operational status.
/// `AttachmentState` represents the user's intent to use a device with AudioMatrix.
///
/// # Zero Auto-Connect Policy
///
/// By default, all devices are `Available` (detected but not attached).
/// User must explicitly call attach to enable routing to/from a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AttachmentState {
    /// Device detected but not attached to AudioMatrix.
    /// No streams allocated, cannot participate in routing.
    #[default]
    Available = 0,
    /// User explicitly attached device for routing.
    /// Streams may be allocated but not necessarily running.
    Attached = 1,
    /// Device is attached and actively streaming audio.
    /// This is a sub-state of Attached when audio callbacks are active.
    Active = 2,
    /// Device was attached but is now detached.
    /// Streams deallocated, routes using this device suspended.
    Detached = 3,
    /// Device has an error preventing attachment.
    Error = 4,
}

impl AttachmentState {
    /// Converts from a u8 value.
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Available),
            1 => Some(Self::Attached),
            2 => Some(Self::Active),
            3 => Some(Self::Detached),
            4 => Some(Self::Error),
            _ => None,
        }
    }

    /// Returns true if the device can participate in routing.
    #[must_use]
    pub const fn can_route(self) -> bool {
        matches!(self, Self::Attached | Self::Active)
    }

    /// Returns true if the device is streaming audio.
    #[must_use]
    pub const fn is_streaming(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns true if the device needs user action to attach.
    #[must_use]
    pub const fn needs_attach(self) -> bool {
        matches!(self, Self::Available | Self::Detached)
    }
}

impl std::fmt::Display for AttachmentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available => write!(f, "available"),
            Self::Attached => write!(f, "attached"),
            Self::Active => write!(f, "active"),
            Self::Detached => write!(f, "detached"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Atomic wrapper for `AttachmentState`.
#[derive(Debug)]
pub struct AtomicAttachmentState {
    value: AtomicU8,
}

impl AtomicAttachmentState {
    /// Creates a new atomic attachment state.
    #[must_use]
    pub fn new(state: AttachmentState) -> Self {
        Self {
            value: AtomicU8::new(state as u8),
        }
    }

    /// Loads the current state.
    #[must_use]
    pub fn load(&self) -> AttachmentState {
        AttachmentState::from_u8(self.value.load(Ordering::Acquire)).unwrap_or_default()
    }

    /// Stores a new state.
    pub fn store(&self, state: AttachmentState) {
        self.value.store(state as u8, Ordering::Release);
    }

    /// Attempts to transition from expected to new state.
    ///
    /// Returns true if the transition was successful.
    pub fn compare_exchange(&self, expected: AttachmentState, new: AttachmentState) -> bool {
        self.value
            .compare_exchange(
                expected as u8,
                new as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

impl Default for AtomicAttachmentState {
    fn default() -> Self {
        Self::new(AttachmentState::default())
    }
}

/// Atomic wrapper for `DeviceState`.
#[derive(Debug)]
pub struct AtomicDeviceState {
    value: AtomicU8,
}

impl AtomicDeviceState {
    /// Creates a new atomic device state.
    #[must_use]
    pub fn new(state: DeviceState) -> Self {
        Self {
            value: AtomicU8::new(state as u8),
        }
    }

    /// Loads the current state.
    #[must_use]
    pub fn load(&self) -> DeviceState {
        DeviceState::from_u8(self.value.load(Ordering::Acquire)).unwrap_or_default()
    }

    /// Stores a new state.
    pub fn store(&self, state: DeviceState) {
        self.value.store(state as u8, Ordering::Release);
    }

    /// Attempts to transition from expected to new state.
    ///
    /// Returns true if the transition was successful.
    pub fn compare_exchange(&self, expected: DeviceState, new: DeviceState) -> bool {
        self.value
            .compare_exchange(
                expected as u8,
                new as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }
}

impl Default for AtomicDeviceState {
    fn default() -> Self {
        Self::new(DeviceState::default())
    }
}

/// Supported sample format for a device configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 16-bit signed integer.
    I16,
    /// 32-bit signed integer.
    I32,
    /// 32-bit floating point.
    F32,
}

impl From<cpal::SampleFormat> for SampleFormat {
    fn from(format: cpal::SampleFormat) -> Self {
        match format {
            cpal::SampleFormat::I16 => Self::I16,
            cpal::SampleFormat::I32 => Self::I32,
            // F32 and any other formats default to F32
            _ => Self::F32,
        }
    }
}

/// Supported device configuration.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Minimum number of channels.
    pub channels_min: u16,
    /// Maximum number of channels.
    pub channels_max: u16,
    /// Minimum sample rate in Hz.
    pub sample_rate_min: u32,
    /// Maximum sample rate in Hz.
    pub sample_rate_max: u32,
    /// Minimum buffer size in samples.
    pub buffer_size_min: u32,
    /// Maximum buffer size in samples.
    pub buffer_size_max: u32,
    /// Supported sample format.
    pub sample_format: SampleFormat,
}

impl DeviceConfig {
    /// Creates a default configuration.
    #[must_use]
    pub fn default_config() -> Self {
        Self {
            channels_min: 1,
            channels_max: 2,
            sample_rate_min: 44100,
            sample_rate_max: 48000,
            buffer_size_min: 64,
            buffer_size_max: 4096,
            sample_format: SampleFormat::F32,
        }
    }
}

/// Information about an audio device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Unique device identifier.
    pub id: String,
    /// Human-readable device name (from driver/OS).
    pub name: String,
    /// User-defined display name (alias).
    /// If None, display `name` instead.
    pub display_name: Option<String>,
    /// Device direction (input/output).
    pub direction: DeviceDirection,
    /// Host API name (ALSA, WASAPI, `CoreAudio`, etc.)
    pub host: String,
    /// Whether this is the default device for its direction.
    pub is_default: bool,
    /// Whether this is a virtual device created by AudioMatrix.
    pub is_virtual: bool,
    /// User-controlled attachment state (zero auto-connect).
    pub attachment_state: AttachmentState,
    /// Supported configurations.
    pub configs: Vec<DeviceConfig>,
    /// Timestamp when device was last seen.
    pub last_seen: Instant,
}

impl DeviceInfo {
    /// Returns true if the device supports the given sample rate.
    #[must_use]
    pub fn supports_sample_rate(&self, rate: u32) -> bool {
        self.configs
            .iter()
            .any(|c| rate >= c.sample_rate_min && rate <= c.sample_rate_max)
    }

    /// Returns true if the device supports the given channel count.
    #[must_use]
    pub fn supports_channels(&self, channels: u16) -> bool {
        self.configs
            .iter()
            .any(|c| channels >= c.channels_min && channels <= c.channels_max)
    }

    /// Returns the maximum supported channel count.
    #[must_use]
    pub fn max_channels(&self) -> u16 {
        self.configs
            .iter()
            .map(|c| c.channels_max)
            .max()
            .unwrap_or(2)
    }

    /// Returns the display name (alias if set, otherwise system name).
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    /// Returns true if the device can participate in routing.
    #[must_use]
    pub fn can_route(&self) -> bool {
        self.attachment_state.can_route()
    }

    /// Returns true if the device is actively streaming.
    #[must_use]
    pub fn is_streaming(&self) -> bool {
        self.attachment_state.is_streaming()
    }
}

/// Event emitted when device state changes.
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// A new device was discovered.
    Added(DeviceInfo),
    /// A device was removed (disconnected).
    Removed { id: String, name: String },
    /// A device's state changed.
    StateChanged {
        id: String,
        old_state: DeviceState,
        new_state: DeviceState,
    },
    /// A device's configuration changed.
    ConfigChanged { id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_state_from_u8() {
        assert_eq!(DeviceState::from_u8(0), Some(DeviceState::Idle));
        assert_eq!(DeviceState::from_u8(1), Some(DeviceState::Starting));
        assert_eq!(DeviceState::from_u8(2), Some(DeviceState::Running));
        assert_eq!(DeviceState::from_u8(3), Some(DeviceState::Error));
        assert_eq!(DeviceState::from_u8(4), Some(DeviceState::Stopped));
        assert_eq!(DeviceState::from_u8(5), Some(DeviceState::Disconnected));
        assert_eq!(DeviceState::from_u8(6), None);
    }

    #[test]
    fn device_state_availability() {
        assert!(DeviceState::Idle.is_available());
        assert!(DeviceState::Stopped.is_available());
        assert!(!DeviceState::Running.is_available());
        assert!(!DeviceState::Error.is_available());
    }

    #[test]
    fn device_state_active() {
        assert!(DeviceState::Starting.is_active());
        assert!(DeviceState::Running.is_active());
        assert!(!DeviceState::Idle.is_active());
        assert!(!DeviceState::Stopped.is_active());
    }

    #[test]
    fn atomic_device_state() {
        let state = AtomicDeviceState::new(DeviceState::Idle);
        assert_eq!(state.load(), DeviceState::Idle);

        state.store(DeviceState::Running);
        assert_eq!(state.load(), DeviceState::Running);

        // Test compare_exchange
        assert!(state.compare_exchange(DeviceState::Running, DeviceState::Stopped));
        assert_eq!(state.load(), DeviceState::Stopped);

        // Should fail - expected doesn't match
        assert!(!state.compare_exchange(DeviceState::Running, DeviceState::Error));
        assert_eq!(state.load(), DeviceState::Stopped);
    }

    #[test]
    fn device_direction_display() {
        assert_eq!(DeviceDirection::Input.to_string(), "input");
        assert_eq!(DeviceDirection::Output.to_string(), "output");
    }

    #[test]
    fn device_state_display() {
        assert_eq!(DeviceState::Idle.to_string(), "idle");
        assert_eq!(DeviceState::Running.to_string(), "running");
        assert_eq!(DeviceState::Error.to_string(), "error");
    }

    #[test]
    fn device_config_default() {
        let config = DeviceConfig::default_config();
        assert_eq!(config.channels_min, 1);
        assert_eq!(config.channels_max, 2);
        assert_eq!(config.sample_rate_min, 44100);
        assert_eq!(config.sample_rate_max, 48000);
    }

    #[test]
    fn device_info_supports_sample_rate() {
        let info = DeviceInfo {
            id: "test".to_string(),
            name: "Test Device".to_string(),
            display_name: None,
            direction: DeviceDirection::Output,
            host: "Test".to_string(),
            is_default: false,
            is_virtual: false,
            attachment_state: AttachmentState::Available,
            configs: vec![DeviceConfig::default_config()],
            last_seen: Instant::now(),
        };

        assert!(info.supports_sample_rate(44100));
        assert!(info.supports_sample_rate(48000));
        assert!(!info.supports_sample_rate(96000));
    }

    #[test]
    fn device_info_supports_channels() {
        let info = DeviceInfo {
            id: "test".to_string(),
            name: "Test Device".to_string(),
            display_name: None,
            direction: DeviceDirection::Output,
            host: "Test".to_string(),
            is_default: false,
            is_virtual: false,
            attachment_state: AttachmentState::Available,
            configs: vec![DeviceConfig::default_config()],
            last_seen: Instant::now(),
        };

        assert!(info.supports_channels(1));
        assert!(info.supports_channels(2));
        assert!(!info.supports_channels(8));
    }

    #[test]
    fn attachment_state_default_is_available() {
        // CRITICAL: Zero auto-connect policy
        assert_eq!(AttachmentState::default(), AttachmentState::Available);
        assert!(AttachmentState::Available.needs_attach());
        assert!(!AttachmentState::Available.can_route());
    }

    #[test]
    fn attachment_state_transitions() {
        assert!(AttachmentState::Attached.can_route());
        assert!(AttachmentState::Active.can_route());
        assert!(AttachmentState::Active.is_streaming());
        assert!(!AttachmentState::Attached.is_streaming());
        assert!(AttachmentState::Detached.needs_attach());
    }
}
