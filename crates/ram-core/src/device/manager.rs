//! Device manager for audio hardware enumeration and state tracking.
//!
//! This module provides:
//! - Cross-platform audio device enumeration via cpal
//! - Device state machine (starting, running, error, stopped)
//! - Hot-plug detection for device arrival/removal
//! - Device cache with TTL for efficient polling

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use parking_lot::RwLock;

use super::types::{
    AtomicDeviceState, AttachmentState, DeviceConfig, DeviceDirection, DeviceEvent, DeviceInfo,
    DeviceState,
};

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
    ///
    /// On Windows, this enumerates devices from ALL available hosts (WASAPI, ASIO, etc.)
    /// On Linux/macOS, this enumerates from the default host (ALSA, `CoreAudio`).
    pub fn refresh(&self) {
        let now = Instant::now();
        let mut current_ids = std::collections::HashSet::new();

        // Get all available audio hosts
        let hosts = cpal::available_hosts();
        tracing::info!("Available audio hosts: {:?}", hosts);

        if hosts.is_empty() {
            tracing::warn!("No audio hosts available - audio subsystem may not be initialized");
        }

        // Enumerate devices from ALL available hosts (WASAPI, ASIO, CoreAudio, ALSA, etc.)
        for host_id in hosts {
            let host = match cpal::host_from_id(host_id) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("Failed to initialize host {:?}: {}", host_id, e);
                    continue;
                },
            };

            let host_name = host_id.name();

            // Get default devices for this host
            let default_input = host.default_input_device().and_then(|d| d.name().ok());
            let default_output = host.default_output_device().and_then(|d| d.name().ok());

            // Enumerate input devices
            match host.input_devices() {
                Ok(devices) => {
                    let mut count = 0;
                    for device in devices {
                        count += 1;
                        match device.name() {
                            Ok(name) => tracing::debug!("Found input device: {}", name),
                            Err(e) => tracing::warn!("Input device has no name: {}", e),
                        }
                        if let Some(info) = Self::device_to_info_with_host(
                            &device,
                            DeviceDirection::Input,
                            default_input.as_ref(),
                            host_name,
                            now,
                        ) {
                            current_ids.insert(info.id.clone());
                            self.update_device(info);
                        }
                    }
                    tracing::info!("Host '{}' has {} input device(s)", host_name, count);
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to enumerate input devices from host '{}': {}",
                        host_name,
                        e
                    );
                },
            }

            // Enumerate output devices
            match host.output_devices() {
                Ok(devices) => {
                    let mut count = 0;
                    for device in devices {
                        count += 1;
                        match device.name() {
                            Ok(name) => tracing::debug!("Found output device: {}", name),
                            Err(e) => tracing::warn!("Output device has no name: {}", e),
                        }
                        if let Some(info) = Self::device_to_info_with_host(
                            &device,
                            DeviceDirection::Output,
                            default_output.as_ref(),
                            host_name,
                            now,
                        ) {
                            current_ids.insert(info.id.clone());
                            self.update_device(info);
                        }
                    }
                    tracing::info!("Host '{}' has {} output device(s)", host_name, count);
                },
                Err(e) => {
                    tracing::warn!(
                        "Failed to enumerate output devices from host '{}': {}",
                        host_name,
                        e
                    );
                },
            }

            tracing::info!(
                "Enumerated from host '{}': {} total devices so far",
                host_name,
                current_ids.len()
            );
        }

        // Detect removed devices
        self.detect_removed_devices(&current_ids);

        *self.last_refresh.write() = now;
    }

    #[allow(dead_code)] // Used in tests
    fn device_to_info(
        device: &cpal::Device,
        direction: DeviceDirection,
        default_name: Option<&String>,
        now: Instant,
    ) -> Option<DeviceInfo> {
        Self::device_to_info_with_host(
            device,
            direction,
            default_name,
            cpal::default_host().id().name(),
            now,
        )
    }

    fn device_to_info_with_host(
        device: &cpal::Device,
        direction: DeviceDirection,
        default_name: Option<&String>,
        host_name: &str,
        now: Instant,
    ) -> Option<DeviceInfo> {
        let name = device.name().ok()?;
        // Include host in ID to differentiate WASAPI vs ASIO devices with same name
        let id = format!("{host_name}:{direction}:{name}");
        let is_default = default_name.is_some_and(|d| d == &name);

        let configs = Self::get_device_configs(device, direction);

        Some(DeviceInfo {
            id,
            name,
            display_name: None, // User can set alias later
            direction,
            host: host_name.to_string(),
            is_default,
            is_virtual: false, // Physical device from cpal
            attachment_state: AttachmentState::Available, // Zero auto-connect
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
