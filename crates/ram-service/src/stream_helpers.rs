//! Stream helper functions for audio processing.
//!
//! This module contains helper functions for finding cpal devices and building streams.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::Stream;
use std::sync::Arc;
use tracing::{debug, error};

use ram_core::callbacks::{
    create_input_callback, create_output_callback, InputCallbackContext, OutputCallbackContext,
};

/// Type alias for cpal stream config.
pub type CpalStreamConfig = cpal::StreamConfig;

/// Direction for device lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceDirection {
    /// Input-only device.
    Input,
    /// Output-only device.
    Output,
    /// Duplex device (input and output).
    Duplex,
}

/// Finds a cpal device by ID and returns the device with its configuration.
///
/// # Arguments
///
/// * `device_id` - Device ID in format "HOST:TYPE:NAME" (e.g., "ASIO:duplex:US-16x08 ASIO")
/// * `direction` - The direction to search for (Input, Output, or Duplex)
///
/// # Returns
///
/// A tuple of the cpal device and its stream configuration.
///
/// # Errors
///
/// Returns an error if the device cannot be found or configuration cannot be obtained.
pub fn find_cpal_device(
    device_id: &str,
    direction: DeviceDirection,
) -> Result<(cpal::Device, CpalStreamConfig)> {
    // Handle virtual devices - they don't use cpal
    if device_id.starts_with("virtual_") {
        return Err(anyhow!(
            "Virtual devices don't use cpal streams: {device_id}"
        ));
    }

    let parts: Vec<&str> = device_id.splitn(3, ':').collect();
    if parts.len() < 3 {
        return Err(anyhow!("Invalid device ID format: {device_id}"));
    }
    let host_name = parts[0];
    let device_type = parts[1]; // "input", "output", or "duplex"
    let device_name = parts[2];

    let hosts = cpal::available_hosts();
    let host_id = hosts
        .iter()
        .find(|h| h.name() == host_name)
        .ok_or_else(|| anyhow!("Host not found: {host_name}"))?;

    let host = cpal::host_from_id(*host_id)
        .map_err(|e| anyhow!("Failed to get host {host_name}: {e}"))?;

    // For duplex devices (like ASIO), we need to search in the appropriate list
    // ASIO devices typically appear in output_devices() even when we want input
    let is_duplex = device_type == "duplex";

    let device = match direction {
        DeviceDirection::Input => {
            // First try input_devices
            let input_device = host
                .input_devices()
                .map_err(|e| anyhow!("Failed to enumerate input devices: {e}"))?
                .find(|d| d.name().ok().as_deref() == Some(device_name));

            if let Some(d) = input_device {
                d
            } else if is_duplex {
                // For duplex devices, also check output_devices (ASIO uses this)
                host.output_devices()
                    .map_err(|e| anyhow!("Failed to enumerate output devices: {e}"))?
                    .find(|d| d.name().ok().as_deref() == Some(device_name))
                    .ok_or_else(|| {
                        anyhow!("Input device not found in input or output list: {device_name}")
                    })?
            } else {
                return Err(anyhow!("Input device not found: {device_name}"));
            }
        }
        DeviceDirection::Output => {
            // First try output_devices
            let output_device = host
                .output_devices()
                .map_err(|e| anyhow!("Failed to enumerate output devices: {e}"))?
                .find(|d| d.name().ok().as_deref() == Some(device_name));

            if let Some(d) = output_device {
                d
            } else if is_duplex {
                // For duplex devices, also check input_devices
                host.input_devices()
                    .map_err(|e| anyhow!("Failed to enumerate input devices: {e}"))?
                    .find(|d| d.name().ok().as_deref() == Some(device_name))
                    .ok_or_else(|| {
                        anyhow!(
                            "Output device not found in output or input list: {device_name}"
                        )
                    })?
            } else {
                return Err(anyhow!("Output device not found: {device_name}"));
            }
        }
        DeviceDirection::Duplex => {
            // For duplex devices, we find in either input or output list
            // (they should be the same physical device)
            host.output_devices()
                .map_err(|e| anyhow!("Failed to enumerate devices: {e}"))?
                .find(|d| d.name().ok().as_deref() == Some(device_name))
                .ok_or_else(|| anyhow!("Duplex device not found: {device_name}"))?
        }
    };

    // Get the appropriate config based on direction
    // For duplex devices (especially ASIO), we may need to try both configs
    let config = match direction {
        DeviceDirection::Input => {
            // Try input config first, fall back to output for duplex ASIO devices
            device.default_input_config().or_else(|e| {
                if is_duplex {
                    debug!(
                        "Input config failed for duplex device, trying output: {e}"
                    );
                    device.default_output_config()
                } else {
                    Err(e)
                }
            })
        }
        DeviceDirection::Output => {
            // Try output config first, fall back to input for duplex devices
            device.default_output_config().or_else(|e| {
                if is_duplex {
                    debug!(
                        "Output config failed for duplex device, trying input: {e}"
                    );
                    device.default_input_config()
                } else {
                    Err(e)
                }
            })
        }
        DeviceDirection::Duplex => device.default_output_config(),
    }
    .map_err(|e| anyhow!("Failed to get config for {device_name}: {e}"))?;

    let stream_config = CpalStreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    Ok((device, stream_config))
}

/// Builds an input stream for the given device and configuration.
///
/// # Arguments
///
/// * `device` - The cpal device to create the stream on
/// * `config` - Stream configuration (sample rate, channels, buffer size)
/// * `context` - The input callback context for processing audio
///
/// # Returns
///
/// The created input stream.
///
/// # Errors
///
/// Returns an error if the stream cannot be built.
pub fn build_input_stream(
    device: &cpal::Device,
    config: &CpalStreamConfig,
    context: Arc<InputCallbackContext>,
) -> Result<Stream> {
    let err_fn = |err| error!("Input stream error: {err}");

    let mut inner_callback = create_input_callback(context);
    let callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
        inner_callback(data);
    };

    let stream = device
        .build_input_stream(config, callback, err_fn, None)
        .map_err(|e| anyhow!("Failed to build input stream: {e}"))?;

    Ok(stream)
}

/// Builds an output stream for the given device and configuration.
///
/// # Arguments
///
/// * `device` - The cpal device to create the stream on
/// * `config` - Stream configuration (sample rate, channels, buffer size)
/// * `context` - The output callback context for processing audio
///
/// # Returns
///
/// The created output stream.
///
/// # Errors
///
/// Returns an error if the stream cannot be built.
pub fn build_output_stream(
    device: &cpal::Device,
    config: &CpalStreamConfig,
    context: Arc<OutputCallbackContext>,
) -> Result<Stream> {
    let err_fn = |err| error!("Output stream error: {err}");

    let mut inner_callback = create_output_callback(context);
    let callback = move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
        inner_callback(data);
    };

    let stream = device
        .build_output_stream(config, callback, err_fn, None)
        .map_err(|e| anyhow!("Failed to build output stream: {e}"))?;

    Ok(stream)
}
