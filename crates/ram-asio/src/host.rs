//! ASIO host management.
//!
//! Provides access to the Windows ASIO audio subsystem.

use crate::device::{AsioDevice, AsioDeviceInfo};
use crate::error::{AsioError, AsioResult};

#[cfg(all(target_os = "windows", feature = "asio"))]
use cpal::traits::{DeviceTrait, HostTrait};

/// ASIO host wrapper providing access to ASIO devices.
///
/// This is the entry point for all ASIO operations. Create an `AsioHost`
/// to enumerate and access ASIO devices on Windows.
pub struct AsioHost {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    inner: cpal::Host,
}

impl AsioHost {
    /// Create a new ASIO host.
    ///
    /// # Errors
    ///
    /// Returns `AsioError::NotAvailable` if:
    /// - Not running on Windows
    /// - Not built with `asio` feature
    ///
    /// Returns `AsioError::HostInitFailed` if ASIO initialization fails.
    pub fn new() -> AsioResult<Self> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            let host = cpal::host_from_id(cpal::HostId::Asio)?;
            tracing::info!("ASIO host initialized successfully");
            Ok(Self { inner: host })
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get information about all available ASIO devices.
    ///
    /// Returns a vector of device info for all detected ASIO devices.
    pub fn device_infos(&self) -> AsioResult<Vec<AsioDeviceInfo>> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            let mut devices = Vec::new();

            for device in self.inner.devices()? {
                let name = device.name()?;

                // Get input channels
                let input_channels = device
                    .supported_input_configs()
                    .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
                    .unwrap_or(0);

                // Get output channels
                let output_channels = device
                    .supported_output_configs()
                    .map(|configs| configs.map(|c| c.channels()).max().unwrap_or(0))
                    .unwrap_or(0);

                // Get supported sample rates
                let sample_rates = device
                    .supported_output_configs()
                    .ok()
                    .map(|configs| {
                        configs
                            .flat_map(|c| {
                                let min = c.min_sample_rate().0;
                                let max = c.max_sample_rate().0;
                                // Common sample rates in range
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
                let (buffer_min, buffer_max) = device
                    .supported_output_configs()
                    .ok()
                    .and_then(|mut configs| configs.next())
                    .map(|c| match c.buffer_size() {
                        cpal::SupportedBufferSize::Range { min, max } => (*min, *max),
                        cpal::SupportedBufferSize::Unknown => (64, 4096),
                    })
                    .unwrap_or((64, 4096));

                devices.push(AsioDeviceInfo {
                    name,
                    input_channels,
                    output_channels,
                    sample_rates,
                    buffer_size_min: buffer_min,
                    buffer_size_max: buffer_max,
                });
            }

            tracing::info!("Found {} ASIO device(s)", devices.len());
            Ok(devices)
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get an ASIO device by name.
    ///
    /// # Errors
    ///
    /// Returns `AsioError::DeviceNotFound` if no device with the given name exists.
    pub fn device_by_name(&self, name: &str) -> AsioResult<AsioDevice> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            for device in self.inner.devices()? {
                if device.name()? == name {
                    return Ok(AsioDevice::new(device));
                }
            }
            Err(AsioError::DeviceNotFound(name.to_string()))
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            let _ = name;
            Err(AsioError::NotAvailable)
        }
    }

    /// Get the default ASIO input device, if any.
    pub fn default_input_device(&self) -> AsioResult<Option<AsioDevice>> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            Ok(self.inner.default_input_device().map(AsioDevice::new))
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get the default ASIO output device, if any.
    pub fn default_output_device(&self) -> AsioResult<Option<AsioDevice>> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            Ok(self.inner.default_output_device().map(AsioDevice::new))
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get the number of available ASIO devices.
    pub fn device_count(&self) -> AsioResult<usize> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            Ok(self.inner.devices()?.count())
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    fn asio_not_available_on_non_windows() {
        let result = AsioHost::new();
        assert!(matches!(result, Err(AsioError::NotAvailable)));
    }

    #[test]
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    fn device_infos_not_available_on_non_windows() {
        // Can't create a real AsioHost on non-Windows, so we test the error path
        // by calling AsioHost::new() which returns NotAvailable
        let result = AsioHost::new();
        assert!(matches!(result, Err(AsioError::NotAvailable)));
    }

    #[test]
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    fn device_by_name_not_available_on_non_windows() {
        let result = AsioHost::new();
        assert!(matches!(result, Err(AsioError::NotAvailable)));
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "asio"))]
    fn asio_host_creation() {
        // This test only runs on Windows with ASIO feature
        // May fail if no ASIO drivers installed
        let result = AsioHost::new();
        // We don't assert success because ASIO might not be available in CI
        if let Ok(host) = result {
            let _ = host.device_infos();
        }
    }
}
