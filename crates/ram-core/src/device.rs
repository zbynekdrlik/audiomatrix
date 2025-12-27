//! Device management for audio hardware enumeration and state tracking.
//!
//! This module provides:
//! - Cross-platform audio device enumeration via cpal
//! - Device state machine (starting, running, error, stopped)
//! - Hot-plug detection for device arrival/removal
//! - Device cache with TTL for efficient polling

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::RwLock;

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
    /// Human-readable device name.
    pub name: String,
    /// Device direction (input/output).
    pub direction: DeviceDirection,
    /// Host API name (ALSA, WASAPI, `CoreAudio`, etc.)
    pub host: String,
    /// Whether this is the default device for its direction.
    pub is_default: bool,
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

/// Configuration for the device manager.
#[derive(Debug, Clone)]
pub struct DeviceManagerConfig {
    /// How often to poll for device changes.
    pub poll_interval: Duration,
    /// TTL for device cache entries.
    pub cache_ttl: Duration,
    /// Whether to automatically refresh on startup.
    pub auto_refresh: bool,
}

impl Default for DeviceManagerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            cache_ttl: Duration::from_secs(10),
            auto_refresh: true,
        }
    }
}

/// Manages audio device enumeration and state.
pub struct DeviceManager {
    /// Configuration.
    config: DeviceManagerConfig,
    /// Cached device information keyed by device ID.
    devices: RwLock<HashMap<String, DeviceInfo>>,
    /// Device states keyed by device ID.
    states: RwLock<HashMap<String, Arc<AtomicDeviceState>>>,
    /// Last refresh timestamp.
    last_refresh: RwLock<Instant>,
    /// Event subscribers (channel senders).
    event_senders: RwLock<Vec<crossbeam_channel::Sender<DeviceEvent>>>,
}

impl DeviceManager {
    /// Creates a new device manager with the given configuration.
    #[must_use]
    pub fn new(config: DeviceManagerConfig) -> Self {
        let manager = Self {
            config,
            devices: RwLock::new(HashMap::new()),
            states: RwLock::new(HashMap::new()),
            last_refresh: RwLock::new(Instant::now()),
            event_senders: RwLock::new(Vec::new()),
        };

        if manager.config.auto_refresh {
            manager.refresh();
        }

        manager
    }

    /// Creates a device manager with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DeviceManagerConfig::default())
    }

    /// Subscribes to device events.
    ///
    /// Returns a receiver that will receive device events.
    pub fn subscribe(&self) -> crossbeam_channel::Receiver<DeviceEvent> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        self.event_senders.write().push(sender);
        receiver
    }

    /// Refreshes the device list by enumerating all available devices.
    pub fn refresh(&self) {
        let host = cpal::default_host();
        let now = Instant::now();
        let mut current_ids = std::collections::HashSet::new();

        // Get default devices
        let default_input = host.default_input_device().and_then(|d| d.name().ok());
        let default_output = host.default_output_device().and_then(|d| d.name().ok());

        // Enumerate input devices
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if let Some(info) = Self::device_to_info(
                    &device,
                    DeviceDirection::Input,
                    default_input.as_ref(),
                    now,
                ) {
                    current_ids.insert(info.id.clone());
                    self.update_device(info);
                }
            }
        }

        // Enumerate output devices
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Some(info) = Self::device_to_info(
                    &device,
                    DeviceDirection::Output,
                    default_output.as_ref(),
                    now,
                ) {
                    current_ids.insert(info.id.clone());
                    self.update_device(info);
                }
            }
        }

        // Detect removed devices
        self.detect_removed_devices(&current_ids);

        *self.last_refresh.write() = now;
    }

    fn device_to_info(
        device: &cpal::Device,
        direction: DeviceDirection,
        default_name: Option<&String>,
        now: Instant,
    ) -> Option<DeviceInfo> {
        let name = device.name().ok()?;
        let id = format!("{direction}:{name}");
        let is_default = default_name.is_some_and(|d| d == &name);

        let configs = Self::get_device_configs(device, direction);

        Some(DeviceInfo {
            id,
            name,
            direction,
            host: cpal::default_host().id().name().to_string(),
            is_default,
            configs,
            last_seen: now,
        })
    }

    fn get_device_configs(device: &cpal::Device, direction: DeviceDirection) -> Vec<DeviceConfig> {
        let configs: Box<dyn Iterator<Item = cpal::SupportedStreamConfigRange>> = match direction {
            DeviceDirection::Input => match device.supported_input_configs() {
                Ok(configs) => Box::new(configs),
                Err(_) => return vec![DeviceConfig::default_config()],
            },
            DeviceDirection::Output => match device.supported_output_configs() {
                Ok(configs) => Box::new(configs),
                Err(_) => return vec![DeviceConfig::default_config()],
            },
        };

        configs
            .map(|config| {
                let buffer_range = config.buffer_size();
                let (buffer_min, buffer_max) = match buffer_range {
                    cpal::SupportedBufferSize::Range { min, max } => (*min, *max),
                    cpal::SupportedBufferSize::Unknown => (64, 4096),
                };

                DeviceConfig {
                    channels_min: config.channels(),
                    channels_max: config.channels(),
                    sample_rate_min: config.min_sample_rate().0,
                    sample_rate_max: config.max_sample_rate().0,
                    buffer_size_min: buffer_min,
                    buffer_size_max: buffer_max,
                    sample_format: config.sample_format().into(),
                }
            })
            .collect()
    }

    fn update_device(&self, info: DeviceInfo) {
        let mut devices = self.devices.write();
        let is_new = !devices.contains_key(&info.id);

        if is_new {
            // Initialize state for new device
            self.states
                .write()
                .insert(info.id.clone(), Arc::new(AtomicDeviceState::default()));

            // Emit added event
            self.emit_event(&DeviceEvent::Added(info.clone()));
        }

        devices.insert(info.id.clone(), info);
    }

    fn detect_removed_devices(&self, current_ids: &std::collections::HashSet<String>) {
        let devices = self.devices.read();
        let mut removed = Vec::new();

        for (id, info) in devices.iter() {
            if !current_ids.contains(id) {
                removed.push((id.clone(), info.name.clone()));
            }
        }

        drop(devices);

        for (id, name) in removed {
            self.remove_device(&id, &name);
        }
    }

    fn remove_device(&self, id: &str, name: &str) {
        self.devices.write().remove(id);

        // Update state to disconnected
        if let Some(state) = self.states.read().get(id) {
            state.store(DeviceState::Disconnected);
        }

        self.emit_event(&DeviceEvent::Removed {
            id: id.to_string(),
            name: name.to_string(),
        });
    }

    fn emit_event(&self, event: &DeviceEvent) {
        let senders = self.event_senders.read();
        // Remove disconnected subscribers
        let mut to_remove = Vec::new();
        for (i, sender) in senders.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                to_remove.push(i);
            }
        }
        drop(senders);

        if !to_remove.is_empty() {
            let mut senders = self.event_senders.write();
            for i in to_remove.into_iter().rev() {
                senders.remove(i);
            }
        }
    }

    /// Returns all cached devices.
    #[must_use]
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.read().values().cloned().collect()
    }

    /// Returns devices filtered by direction.
    #[must_use]
    pub fn devices_by_direction(&self, direction: DeviceDirection) -> Vec<DeviceInfo> {
        self.devices
            .read()
            .values()
            .filter(|d| d.direction == direction)
            .cloned()
            .collect()
    }

    /// Returns input devices.
    #[must_use]
    pub fn input_devices(&self) -> Vec<DeviceInfo> {
        self.devices_by_direction(DeviceDirection::Input)
    }

    /// Returns output devices.
    #[must_use]
    pub fn output_devices(&self) -> Vec<DeviceInfo> {
        self.devices_by_direction(DeviceDirection::Output)
    }

    /// Gets a device by ID.
    #[must_use]
    pub fn get_device(&self, id: &str) -> Option<DeviceInfo> {
        self.devices.read().get(id).cloned()
    }

    /// Gets the default input device.
    #[must_use]
    pub fn default_input(&self) -> Option<DeviceInfo> {
        self.devices
            .read()
            .values()
            .find(|d| d.direction == DeviceDirection::Input && d.is_default)
            .cloned()
    }

    /// Gets the default output device.
    #[must_use]
    pub fn default_output(&self) -> Option<DeviceInfo> {
        self.devices
            .read()
            .values()
            .find(|d| d.direction == DeviceDirection::Output && d.is_default)
            .cloned()
    }

    /// Gets the state of a device.
    #[must_use]
    pub fn device_state(&self, id: &str) -> Option<DeviceState> {
        self.states.read().get(id).map(|s| s.load())
    }

    /// Sets the state of a device.
    ///
    /// Emits a `StateChanged` event if the state actually changed.
    pub fn set_device_state(&self, id: &str, new_state: DeviceState) {
        let states = self.states.read();
        if let Some(state) = states.get(id) {
            let old_state = state.load();
            if old_state != new_state {
                state.store(new_state);
                drop(states);
                self.emit_event(&DeviceEvent::StateChanged {
                    id: id.to_string(),
                    old_state,
                    new_state,
                });
            }
        }
    }

    /// Attempts to transition a device from one state to another.
    ///
    /// Returns true if the transition was successful.
    pub fn transition_device_state(
        &self,
        id: &str,
        expected: DeviceState,
        new: DeviceState,
    ) -> bool {
        let states = self.states.read();
        if let Some(state) = states.get(id) {
            if state.compare_exchange(expected, new) {
                drop(states);
                self.emit_event(&DeviceEvent::StateChanged {
                    id: id.to_string(),
                    old_state: expected,
                    new_state: new,
                });
                return true;
            }
        }
        false
    }

    /// Returns the time since the last refresh.
    #[must_use]
    pub fn time_since_refresh(&self) -> Duration {
        self.last_refresh.read().elapsed()
    }

    /// Returns true if the cache is stale (older than TTL).
    #[must_use]
    pub fn is_cache_stale(&self) -> bool {
        self.time_since_refresh() > self.config.cache_ttl
    }

    /// Refreshes if the cache is stale.
    pub fn refresh_if_stale(&self) {
        if self.is_cache_stale() {
            self.refresh();
        }
    }

    /// Returns the number of cached devices.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.read().len()
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::with_defaults()
    }
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
            direction: DeviceDirection::Output,
            host: "Test".to_string(),
            is_default: false,
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
            direction: DeviceDirection::Output,
            host: "Test".to_string(),
            is_default: false,
            configs: vec![DeviceConfig::default_config()],
            last_seen: Instant::now(),
        };

        assert!(info.supports_channels(1));
        assert!(info.supports_channels(2));
        assert!(!info.supports_channels(8));
    }

    #[test]
    fn device_manager_config_default() {
        let config = DeviceManagerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(2));
        assert_eq!(config.cache_ttl, Duration::from_secs(10));
        assert!(config.auto_refresh);
    }

    #[test]
    fn device_manager_creation() {
        let config = DeviceManagerConfig {
            auto_refresh: false, // Don't enumerate on creation for test
            ..Default::default()
        };
        let manager = DeviceManager::new(config);

        // Should start empty when auto_refresh is disabled
        assert_eq!(manager.device_count(), 0);
    }

    #[test]
    fn device_manager_subscribe() {
        let config = DeviceManagerConfig {
            auto_refresh: false,
            ..Default::default()
        };
        let manager = DeviceManager::new(config);
        let _receiver = manager.subscribe();

        // Just verify subscription works
    }

    #[test]
    #[cfg_attr(windows, ignore = "cpal cleanup causes access violation on Windows CI")]
    fn device_manager_refresh() {
        let manager = DeviceManager::with_defaults();

        // After refresh, we should have discovered devices
        // (or none if running in CI without audio hardware)
        let _ = manager.devices();

        // Verify refresh updates timestamp
        let before = manager.time_since_refresh();
        std::thread::sleep(Duration::from_millis(10));
        manager.refresh();
        let after = manager.time_since_refresh();
        assert!(after < before);
    }

    #[test]
    fn device_manager_cache_staleness() {
        let config = DeviceManagerConfig {
            cache_ttl: Duration::from_millis(100),
            auto_refresh: false,
            ..Default::default()
        };
        let manager = DeviceManager::new(config);

        // Simulate a refresh by directly updating the last_refresh time
        *manager.last_refresh.write() = Instant::now();

        assert!(!manager.is_cache_stale());

        std::thread::sleep(Duration::from_millis(150));

        assert!(manager.is_cache_stale());
    }

    #[test]
    #[cfg_attr(windows, ignore = "cpal cleanup causes access violation on Windows CI")]
    fn device_manager_by_direction() {
        let manager = DeviceManager::with_defaults();

        let inputs = manager.input_devices();
        let outputs = manager.output_devices();

        // All inputs should have Input direction
        for device in &inputs {
            assert_eq!(device.direction, DeviceDirection::Input);
        }

        // All outputs should have Output direction
        for device in &outputs {
            assert_eq!(device.direction, DeviceDirection::Output);
        }
    }
}
