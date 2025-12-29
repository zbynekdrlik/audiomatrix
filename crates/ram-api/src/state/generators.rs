//! Wave generator management for AppState.
//!
//! This module provides functions for managing per-channel wave generators
//! used for test signal injection.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::models::{DeviceInfo, GeneratorStatus, WaveformType};

/// Get generator status for all output channels of a device.
pub fn get_generator_status(
    generators: &RwLock<HashMap<String, Arc<ram_core::WaveGeneratorConfig>>>,
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    device_id: &str,
) -> Vec<GeneratorStatus> {
    let generators = generators.read();
    let devices = devices.read();

    // Get channel count from device
    let channel_count = devices
        .get(device_id)
        .map(|d| d.output_channels)
        .unwrap_or(0);

    // Return status for each channel
    (1..=channel_count)
        .map(|ch| {
            let key = format!("{device_id}:{ch}");
            generators.get(&key).map_or_else(
                || GeneratorStatus {
                    channel: ch,
                    enabled: false,
                    waveform: WaveformType::default(),
                    frequency: 1000,
                    level_db: -18.0,
                },
                |cfg| GeneratorStatus {
                    channel: ch,
                    enabled: cfg.is_enabled(),
                    waveform: WaveformType::from_core(cfg.waveform()),
                    frequency: cfg.frequency(),
                    level_db: cfg.level_db(),
                },
            )
        })
        .collect()
}

/// Set generator for a specific channel.
pub fn set_channel_generator(
    generators: &RwLock<HashMap<String, Arc<ram_core::WaveGeneratorConfig>>>,
    device_id: &str,
    channel: u16,
    enabled: bool,
    waveform: WaveformType,
    frequency: u32,
    level_db: f32,
) {
    let key = format!("{device_id}:{channel}");
    let mut generators = generators.write();

    let config = generators
        .entry(key)
        .or_insert_with(|| Arc::new(ram_core::WaveGeneratorConfig::default()));

    config.set_enabled(enabled);
    config.set_waveform(waveform.to_core());
    config.set_frequency(frequency);
    config.set_level_db(level_db);
}

/// Set generator for all output channels of a device.
pub fn set_all_generators(
    generators: &RwLock<HashMap<String, Arc<ram_core::WaveGeneratorConfig>>>,
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    device_id: &str,
    enabled: bool,
    waveform: WaveformType,
    frequency: u32,
    level_db: f32,
) {
    let devices_guard = devices.read();

    // Get channel count from device
    let channel_count = devices_guard
        .get(device_id)
        .map(|d| d.output_channels)
        .unwrap_or(0);

    drop(devices_guard); // Release read lock before acquiring write lock

    let mut generators = generators.write();

    for ch in 1..=channel_count {
        let key = format!("{device_id}:{ch}");
        let config = generators
            .entry(key)
            .or_insert_with(|| Arc::new(ram_core::WaveGeneratorConfig::default()));

        config.set_enabled(enabled);
        config.set_waveform(waveform.to_core());
        config.set_frequency(frequency);
        config.set_level_db(level_db);
    }
}
