//! ASIO device types and operations.
//!
//! Provides abstraction over ASIO audio devices for both input and output.

use crate::error::{AsioError, AsioResult};

#[cfg(all(target_os = "windows", feature = "asio"))]
use cpal::traits::DeviceTrait;

/// Information about an ASIO device.
///
/// This struct contains static information about a device that doesn't
/// require the device to be opened.
#[derive(Debug, Clone)]
pub struct AsioDeviceInfo {
    /// Device name as reported by the ASIO driver.
    pub name: String,
    /// Maximum number of input channels.
    pub input_channels: u16,
    /// Maximum number of output channels.
    pub output_channels: u16,
    /// Supported sample rates in Hz.
    pub sample_rates: Vec<u32>,
    /// Minimum supported buffer size in samples.
    pub buffer_size_min: u32,
    /// Maximum supported buffer size in samples.
    pub buffer_size_max: u32,
}

impl AsioDeviceInfo {
    /// Check if this device supports a given sample rate.
    #[must_use]
    pub fn supports_sample_rate(&self, rate: u32) -> bool {
        self.sample_rates.contains(&rate)
    }

    /// Check if a buffer size is within the supported range.
    #[must_use]
    pub fn supports_buffer_size(&self, size: u32) -> bool {
        size >= self.buffer_size_min && size <= self.buffer_size_max
    }

    /// Check if this device has input capability.
    #[must_use]
    pub fn has_input(&self) -> bool {
        self.input_channels > 0
    }

    /// Check if this device has output capability.
    #[must_use]
    pub fn has_output(&self) -> bool {
        self.output_channels > 0
    }
}

/// Configuration for opening an ASIO device.
#[derive(Debug, Clone)]
pub struct AsioDeviceConfig {
    /// Desired sample rate in Hz.
    pub sample_rate: u32,
    /// Desired buffer size in samples.
    pub buffer_size: u32,
    /// Number of input channels to use (0 to disable input).
    pub input_channels: u16,
    /// Number of output channels to use (0 to disable output).
    pub output_channels: u16,
}

impl Default for AsioDeviceConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 256,
            input_channels: 2,
            output_channels: 2,
        }
    }
}

/// An opened ASIO device ready for streaming.
///
/// This wraps a cpal device and provides ASIO-specific functionality.
pub struct AsioDevice {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    inner: cpal::Device,
    name: String,
}

impl AsioDevice {
    /// Create a new AsioDevice wrapper.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub(crate) fn new(device: cpal::Device) -> Self {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        Self {
            inner: device,
            name,
        }
    }

    /// Get the device name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get detailed information about this device.
    pub fn info(&self) -> AsioResult<AsioDeviceInfo> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            // Get input channels
            let input_channels = self
                .inner
                .supported_input_configs()
                .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
                .unwrap_or(0);

            // Get output channels
            let output_channels = self
                .inner
                .supported_output_configs()
                .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
                .unwrap_or(0);

            // Get supported sample rates
            let sample_rates = self
                .inner
                .supported_output_configs()
                .ok()
                .map(|configs| {
                    configs
                        .flat_map(|c| {
                            let min = c.min_sample_rate().0;
                            let max = c.max_sample_rate().0;
                            [44100, 48000, 88200, 96000, 176400, 192000]
                                .into_iter()
                                .filter(move |&r| r >= min && r <= max)
                        })
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect()
                })
                .unwrap_or_default();

            // Get buffer size range
            let (buffer_min, buffer_max) = self
                .inner
                .supported_output_configs()
                .ok()
                .and_then(|mut configs| configs.next())
                .map(|c| match c.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => (*min, *max),
                    cpal::SupportedBufferSize::Unknown => (64, 4096),
                })
                .unwrap_or((64, 4096));

            Ok(AsioDeviceInfo {
                name: self.name.clone(),
                input_channels,
                output_channels,
                sample_rates,
                buffer_size_min: buffer_min,
                buffer_size_max: buffer_max,
            })
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get the underlying cpal device (Windows with ASIO only).
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub(crate) fn cpal_device(&self) -> &cpal::Device {
        &self.inner
    }

    /// Get supported input configurations.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn supported_input_configs(
        &self,
    ) -> AsioResult<impl Iterator<Item = cpal::SupportedStreamConfigRange>> {
        self.inner
            .supported_input_configs()
            .map_err(AsioError::from)
    }

    /// Get supported output configurations.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn supported_output_configs(
        &self,
    ) -> AsioResult<impl Iterator<Item = cpal::SupportedStreamConfigRange>> {
        self.inner
            .supported_output_configs()
            .map_err(AsioError::from)
    }

    /// Get the default input configuration.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn default_input_config(&self) -> AsioResult<cpal::SupportedStreamConfig> {
        self.inner
            .default_input_config()
            .map_err(|e| AsioError::ConfigError(e.to_string()))
    }

    /// Get the default output configuration.
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn default_output_config(&self) -> AsioResult<cpal::SupportedStreamConfig> {
        self.inner
            .default_output_config()
            .map_err(|e| AsioError::ConfigError(e.to_string()))
    }
}

impl std::fmt::Debug for AsioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsioDevice")
            .field("name", &self.name)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_supports_sample_rate() {
        let info = AsioDeviceInfo {
            name: "Test".to_string(),
            input_channels: 2,
            output_channels: 2,
            sample_rates: vec![44100, 48000, 96000],
            buffer_size_min: 64,
            buffer_size_max: 4096,
        };

        assert!(info.supports_sample_rate(48000));
        assert!(!info.supports_sample_rate(22050));
    }

    #[test]
    fn device_info_supports_buffer_size() {
        let info = AsioDeviceInfo {
            name: "Test".to_string(),
            input_channels: 2,
            output_channels: 2,
            sample_rates: vec![48000],
            buffer_size_min: 64,
            buffer_size_max: 4096,
        };

        assert!(info.supports_buffer_size(256));
        assert!(info.supports_buffer_size(64));
        assert!(info.supports_buffer_size(4096));
        assert!(!info.supports_buffer_size(32));
        assert!(!info.supports_buffer_size(8192));
    }

    #[test]
    fn device_info_has_input_output() {
        let input_only = AsioDeviceInfo {
            name: "Input".to_string(),
            input_channels: 2,
            output_channels: 0,
            sample_rates: vec![48000],
            buffer_size_min: 64,
            buffer_size_max: 4096,
        };

        assert!(input_only.has_input());
        assert!(!input_only.has_output());

        let output_only = AsioDeviceInfo {
            name: "Output".to_string(),
            input_channels: 0,
            output_channels: 2,
            sample_rates: vec![48000],
            buffer_size_min: 64,
            buffer_size_max: 4096,
        };

        assert!(!output_only.has_input());
        assert!(output_only.has_output());
    }

    #[test]
    fn default_config() {
        let config = AsioDeviceConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.buffer_size, 256);
        assert_eq!(config.input_channels, 2);
        assert_eq!(config.output_channels, 2);
    }
}
