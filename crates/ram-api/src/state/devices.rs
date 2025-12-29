//! Device management for AppState.
//!
//! This module provides functions for managing device attachment,
//! channel labels, and virtual devices.

use std::collections::HashMap;

use parking_lot::RwLock;

use crate::models::{ChannelInfo, DeviceInfo, DeviceStatus, DeviceType};

/// Attaches a device for routing.
pub fn attach_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    id: &str,
    display_name: Option<String>,
) -> Result<DeviceInfo, String> {
    let mut devices = devices.write();
    let device = devices
        .get_mut(id)
        .ok_or_else(|| format!("Device not found: {id}"))?;

    match device.status {
        DeviceStatus::Attached | DeviceStatus::Active => {
            return Err(format!("Device already attached: {id}"));
        },
        DeviceStatus::Error => {
            return Err(format!("Device in error state: {id}"));
        },
        _ => {},
    }

    device.status = DeviceStatus::Attached;
    if display_name.is_some() {
        device.display_name = display_name;
    }

    Ok(device.clone())
}

/// Detaches a device from routing.
pub fn detach_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    id: &str,
) -> Result<DeviceInfo, String> {
    let mut devices = devices.write();
    let device = devices
        .get_mut(id)
        .ok_or_else(|| format!("Device not found: {id}"))?;

    match device.status {
        DeviceStatus::Available | DeviceStatus::Detached => {
            return Err(format!("Device not attached: {id}"));
        },
        DeviceStatus::Error => {
            return Err(format!("Device in error state: {id}"));
        },
        _ => {},
    }

    device.status = DeviceStatus::Detached;
    Ok(device.clone())
}

/// Updates device settings.
pub fn update_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    id: &str,
    display_name: Option<String>,
) -> Result<DeviceInfo, String> {
    let mut devices = devices.write();
    let device = devices
        .get_mut(id)
        .ok_or_else(|| format!("Device not found: {id}"))?;

    if let Some(name) = display_name {
        device.display_name = if name.is_empty() { None } else { Some(name) };
    }

    Ok(device.clone())
}

/// Gets channel information for a device.
pub fn get_device_channels(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    channel_labels: &RwLock<HashMap<String, HashMap<u16, String>>>,
    device_id: &str,
) -> Option<Vec<ChannelInfo>> {
    let devices = devices.read();
    let device = devices.get(device_id)?;

    let labels = channel_labels.read();
    let device_labels = labels.get(device_id);

    let channel_count = device.input_channels.max(device.output_channels);
    let channels = (1..=channel_count)
        .map(|n| ChannelInfo {
            number: n,
            label: device_labels.and_then(|l| l.get(&n).cloned()),
            level_dbfs: None,
            peak_dbfs: None,
        })
        .collect();

    Some(channels)
}

/// Sets a channel label.
pub fn set_channel_label(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    channel_labels: &RwLock<HashMap<String, HashMap<u16, String>>>,
    device_id: &str,
    channel: u16,
    label: String,
) -> Result<ChannelInfo, String> {
    // Verify device exists
    {
        let devices = devices.read();
        if !devices.contains_key(device_id) {
            return Err(format!("Device not found: {device_id}"));
        }
    }

    // Validate label length (31 chars max for VBAN compatibility)
    if label.len() > 31 {
        return Err("Label too long (max 31 characters)".to_string());
    }

    let mut labels = channel_labels.write();
    let device_labels = labels.entry(device_id.to_string()).or_default();

    if label.is_empty() {
        device_labels.remove(&channel);
    } else {
        device_labels.insert(channel, label.clone());
    }

    Ok(ChannelInfo {
        number: channel,
        label: if label.is_empty() { None } else { Some(label) },
        level_dbfs: None,
        peak_dbfs: None,
    })
}

/// Sets multiple channel labels at once.
pub fn set_channel_labels(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    channel_labels: &RwLock<HashMap<String, HashMap<u16, String>>>,
    device_id: &str,
    labels_map: HashMap<u16, String>,
) -> Result<Vec<ChannelInfo>, String> {
    // Verify device exists
    let channel_count = {
        let devices = devices.read();
        let device = devices
            .get(device_id)
            .ok_or_else(|| format!("Device not found: {device_id}"))?;
        device.input_channels.max(device.output_channels)
    };

    // Validate all labels
    for label in labels_map.values() {
        if label.len() > 31 {
            return Err("Label too long (max 31 characters)".to_string());
        }
    }

    // Apply labels
    let mut labels = channel_labels.write();
    let device_labels = labels.entry(device_id.to_string()).or_default();

    for (channel, label) in &labels_map {
        if label.is_empty() {
            device_labels.remove(channel);
        } else {
            device_labels.insert(*channel, label.clone());
        }
    }

    // Build response
    let result = (1..=channel_count)
        .map(|n| ChannelInfo {
            number: n,
            label: device_labels.get(&n).cloned(),
            level_dbfs: None,
            peak_dbfs: None,
        })
        .collect();

    Ok(result)
}

/// Creates a virtual device.
#[allow(clippy::too_many_arguments)]
pub fn create_virtual_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    name: String,
    input_channels: u16,
    output_channels: u16,
    sample_rate: u32,
    buffer_size: u32,
) -> Result<DeviceInfo, String> {
    // Generate a unique ID for the virtual device
    let id = format!("virtual_{}", uuid::Uuid::new_v4().simple());

    // Determine device type
    let device_type = if input_channels > 0 && output_channels > 0 {
        DeviceType::Duplex
    } else if input_channels > 0 {
        DeviceType::Input
    } else {
        DeviceType::Output
    };

    let device = DeviceInfo {
        id: id.clone(),
        name: name.clone(),
        display_name: Some(name),
        device_type,
        input_channels,
        output_channels,
        sample_rate,
        buffer_size,
        is_virtual: true,
        status: DeviceStatus::Available,
        backend: Some("Virtual".to_string()),
    };

    devices.write().insert(id, device.clone());

    Ok(device)
}

/// Updates a virtual device.
#[allow(clippy::too_many_arguments)]
pub fn update_virtual_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    id: &str,
    name: Option<String>,
    input_channels: Option<u16>,
    output_channels: Option<u16>,
    sample_rate: Option<u32>,
    buffer_size: Option<u32>,
) -> Result<DeviceInfo, String> {
    let mut devices = devices.write();
    let device = devices
        .get_mut(id)
        .ok_or_else(|| format!("Device not found: {id}"))?;

    if !device.is_virtual {
        return Err(format!("Device is not a virtual device: {id}"));
    }

    if let Some(n) = name {
        device.name = n.clone();
        device.display_name = Some(n);
    }
    if let Some(ic) = input_channels {
        device.input_channels = ic;
    }
    if let Some(oc) = output_channels {
        device.output_channels = oc;
    }
    if let Some(sr) = sample_rate {
        device.sample_rate = sr;
    }
    if let Some(bs) = buffer_size {
        device.buffer_size = bs;
    }

    // Update device type based on channels
    device.device_type = if device.input_channels > 0 && device.output_channels > 0 {
        DeviceType::Duplex
    } else if device.input_channels > 0 {
        DeviceType::Input
    } else {
        DeviceType::Output
    };

    Ok(device.clone())
}

/// Deletes a virtual device.
pub fn delete_virtual_device(
    devices: &RwLock<HashMap<String, DeviceInfo>>,
    id: &str,
) -> Result<DeviceInfo, String> {
    let mut devices = devices.write();

    // First check if device exists and is virtual
    {
        let device = devices
            .get(id)
            .ok_or_else(|| format!("Device not found: {id}"))?;
        if !device.is_virtual {
            return Err(format!("Device is not a virtual device: {id}"));
        }
    }

    // Remove and return
    devices
        .remove(id)
        .ok_or_else(|| format!("Device not found: {id}"))
}
