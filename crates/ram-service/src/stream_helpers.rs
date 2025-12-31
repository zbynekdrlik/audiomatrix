//! Stream helper functions for audio processing.
//!
//! This module contains helper functions for finding cpal devices and building streams.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, SampleFormat, Stream};
use std::sync::Arc;
use tracing::{debug, error, info};

use ram_core::callbacks::{
    create_input_callback, create_output_callback, InputCallbackContext, OutputCallbackContext,
};

/// Type alias for cpal stream config.
pub type CpalStreamConfig = cpal::StreamConfig;

/// Extended stream config that includes sample format.
#[derive(Debug, Clone)]
pub struct ExtendedStreamConfig {
    /// Base cpal stream config.
    pub config: CpalStreamConfig,
    /// Sample format required by the device.
    pub sample_format: SampleFormat,
}

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
/// A tuple of the cpal device and its extended stream configuration (including sample format).
///
/// # Errors
///
/// Returns an error if the device cannot be found or configuration cannot be obtained.
pub fn find_cpal_device(
    device_id: &str,
    direction: DeviceDirection,
) -> Result<(cpal::Device, ExtendedStreamConfig)> {
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

    let sample_format = config.sample_format();
    info!(
        "Device {device_name}: {} channels, {}Hz, format {:?}",
        config.channels(),
        config.sample_rate().0,
        sample_format
    );

    let stream_config = CpalStreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    Ok((device, ExtendedStreamConfig {
        config: stream_config,
        sample_format,
    }))
}

/// Builds an input stream for the given device and configuration.
///
/// # Arguments
///
/// * `device` - The cpal device to create the stream on
/// * `config` - Stream configuration (sample rate, channels, buffer size)
/// * `sample_format` - The sample format required by the device
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
    sample_format: SampleFormat,
    context: Arc<InputCallbackContext>,
) -> Result<Stream> {
    let err_fn = |err| error!("Input stream error: {err}");

    match sample_format {
        SampleFormat::F32 => {
            let mut inner_callback = create_input_callback(context);
            let callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                inner_callback(data);
            };
            device
                .build_input_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build F32 input stream: {e}"))
        }
        SampleFormat::I32 => {
            let mut inner_callback = create_input_callback(context);
            let callback = move |data: &[i32], _info: &cpal::InputCallbackInfo| {
                // Convert i32 samples to f32
                let f32_data: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                inner_callback(&f32_data);
            };
            device
                .build_input_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build I32 input stream: {e}"))
        }
        SampleFormat::I16 => {
            let mut inner_callback = create_input_callback(context);
            let callback = move |data: &[i16], _info: &cpal::InputCallbackInfo| {
                // Convert i16 samples to f32
                let f32_data: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                inner_callback(&f32_data);
            };
            device
                .build_input_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build I16 input stream: {e}"))
        }
        SampleFormat::U8 => {
            let mut inner_callback = create_input_callback(context);
            let callback = move |data: &[u8], _info: &cpal::InputCallbackInfo| {
                // Convert u8 samples to f32 (centered at 128)
                let f32_data: Vec<f32> = data.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect();
                inner_callback(&f32_data);
            };
            device
                .build_input_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build U8 input stream: {e}"))
        }
        format => Err(anyhow!("Unsupported sample format: {:?}", format)),
    }
}

/// Builds an output stream for the given device and configuration.
///
/// # Arguments
///
/// * `device` - The cpal device to create the stream on
/// * `config` - Stream configuration (sample rate, channels, buffer size)
/// * `sample_format` - The sample format required by the device
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
    sample_format: SampleFormat,
    context: Arc<OutputCallbackContext>,
) -> Result<Stream> {
    let err_fn = |err| error!("Output stream error: {err}");

    match sample_format {
        SampleFormat::F32 => {
            let mut inner_callback = create_output_callback(context);
            let callback = move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                inner_callback(data);
            };
            device
                .build_output_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build F32 output stream: {e}"))
        }
        SampleFormat::I32 => {
            let mut inner_callback = create_output_callback(context);
            let callback = move |data: &mut [i32], _info: &cpal::OutputCallbackInfo| {
                // Get f32 samples and convert to i32
                let mut f32_data: Vec<f32> = vec![0.0; data.len()];
                inner_callback(&mut f32_data);
                for (out, &sample) in data.iter_mut().zip(f32_data.iter()) {
                    *out = i32::from_sample(sample);
                }
            };
            device
                .build_output_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build I32 output stream: {e}"))
        }
        SampleFormat::I16 => {
            let mut inner_callback = create_output_callback(context);
            let callback = move |data: &mut [i16], _info: &cpal::OutputCallbackInfo| {
                // Get f32 samples and convert to i16
                let mut f32_data: Vec<f32> = vec![0.0; data.len()];
                inner_callback(&mut f32_data);
                for (out, &sample) in data.iter_mut().zip(f32_data.iter()) {
                    *out = i16::from_sample(sample);
                }
            };
            device
                .build_output_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build I16 output stream: {e}"))
        }
        SampleFormat::U8 => {
            let mut inner_callback = create_output_callback(context);
            let callback = move |data: &mut [u8], _info: &cpal::OutputCallbackInfo| {
                // Get f32 samples and convert to u8 (centered at 128)
                let mut f32_data: Vec<f32> = vec![0.0; data.len()];
                inner_callback(&mut f32_data);
                for (out, &sample) in data.iter_mut().zip(f32_data.iter()) {
                    *out = ((sample * 128.0) + 128.0).clamp(0.0, 255.0) as u8;
                }
            };
            device
                .build_output_stream(config, callback, err_fn, None)
                .map_err(|e| anyhow!("Failed to build U8 output stream: {e}"))
        }
        format => Err(anyhow!("Unsupported sample format: {:?}", format)),
    }
}
