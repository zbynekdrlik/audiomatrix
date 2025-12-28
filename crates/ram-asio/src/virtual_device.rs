//! Virtual ASIO device management.
//!
//! Provides the Rust side of the virtual ASIO driver communication.
//! Creates and manages shared memory regions that the C++ ASIO driver connects to.

use crate::error::{AsioError, AsioResult};

/// Magic number for shared memory validation: "AUDIOMTX"
#[cfg(target_os = "windows")]
const SHARED_MEMORY_MAGIC: u64 = 0x4155_4449_4F4D_5458;

/// Protocol version
#[cfg(target_os = "windows")]
const SHARED_MEMORY_VERSION: u32 = 1;

/// Shared memory header - must match C++ definition exactly.
///
/// Note: This is a packed struct for binary compatibility with C++.
/// Cannot derive Debug due to atomic fields in packed struct.
#[repr(C, packed)]
pub struct SharedMemoryHeader {
    /// Magic number for validation
    pub magic: u64,
    /// Protocol version
    pub version: u32,
    /// Number of channels
    pub channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Buffer size in samples per channel
    pub buffer_size: u32,
    /// Ring buffer read position (atomic)
    pub read_pos: u64,
    /// Ring buffer write position (atomic)
    pub write_pos: u64,
    /// Status flags
    pub flags: u64,
    /// Reserved for future use
    pub reserved: [u8; 16],
}

impl std::fmt::Debug for SharedMemoryHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Safe to read packed fields by copying
        f.debug_struct("SharedMemoryHeader")
            .field("magic", &{ self.magic })
            .field("version", &{ self.version })
            .field("channels", &{ self.channels })
            .field("sample_rate", &{ self.sample_rate })
            .field("buffer_size", &{ self.buffer_size })
            .finish()
    }
}

/// Flags for shared memory status
pub mod flags {
    /// AudioMatrix service is connected
    pub const SERVICE_CONNECTED: u64 = 1 << 0;
    /// ASIO driver is running
    pub const DRIVER_RUNNING: u64 = 1 << 1;
    /// Buffer switch requested
    pub const BUFFER_SWITCH_REQ: u64 = 1 << 2;
}

/// Configuration for a virtual ASIO device.
#[derive(Debug, Clone)]
pub struct VirtualDeviceConfig {
    /// Device name (will be shown in DAWs)
    pub name: String,
    /// Number of input channels
    pub input_channels: u32,
    /// Number of output channels
    pub output_channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Buffer size in samples
    pub buffer_size: u32,
}

impl Default for VirtualDeviceConfig {
    fn default() -> Self {
        Self {
            name: "AudioMatrix Virtual".to_string(),
            input_channels: 2,
            output_channels: 2,
            sample_rate: 48000,
            buffer_size: 256,
        }
    }
}

/// A virtual ASIO device managed by AudioMatrix.
///
/// Creates shared memory that the C++ ASIO driver connects to,
/// enabling DAWs to route audio through AudioMatrix.
#[cfg(target_os = "windows")]
pub struct VirtualDevice {
    config: VirtualDeviceConfig,
    #[allow(dead_code)]
    shared_memory: SharedMemoryRegion,
}

#[cfg(target_os = "windows")]
impl VirtualDevice {
    /// Create a new virtual ASIO device.
    ///
    /// This creates the shared memory region that the ASIO driver
    /// will connect to when a DAW opens the device.
    pub fn new(config: VirtualDeviceConfig) -> AsioResult<Self> {
        let shared_memory = SharedMemoryRegion::create(&config)?;

        Ok(Self {
            config,
            shared_memory,
        })
    }

    /// Get the device configuration.
    #[must_use]
    pub fn config(&self) -> &VirtualDeviceConfig {
        &self.config
    }

    /// Check if an ASIO driver is connected.
    #[must_use]
    pub fn is_driver_connected(&self) -> bool {
        self.shared_memory.is_driver_connected()
    }

    /// Check if the driver is actively running (processing audio).
    #[must_use]
    pub fn is_driver_running(&self) -> bool {
        self.shared_memory.is_driver_running()
    }
}

/// Shared memory region for virtual device IPC.
#[cfg(target_os = "windows")]
struct SharedMemoryRegion {
    handle: windows::Win32::Foundation::HANDLE,
    ptr: *mut std::ffi::c_void,
    size: usize,
}

#[cfg(target_os = "windows")]
impl SharedMemoryRegion {
    /// Default ring buffer size (64K samples per channel)
    const DEFAULT_RING_SIZE: usize = 65536;

    fn create(config: &VirtualDeviceConfig) -> AsioResult<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Memory::{
            CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
        };

        let channels = config.input_channels.max(config.output_channels) as usize;
        let ring_size = Self::DEFAULT_RING_SIZE * channels * std::mem::size_of::<f32>();
        let total_size = std::mem::size_of::<SharedMemoryHeader>() + ring_size * 2;

        // Create shared memory name
        let name = format!("Local\\AudioMatrix_VASIO_{}", config.name.replace(' ', "_"));
        let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            // Create file mapping
            let handle = CreateFileMappingW(
                windows::Win32::Foundation::INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                total_size as u32,
                PCWSTR(name_wide.as_ptr()),
            )
            .map_err(|e| AsioError::ConfigError(format!("CreateFileMapping failed: {e}")))?;

            // Map view
            let ptr = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, total_size);

            if ptr.Value.is_null() {
                let _ = CloseHandle(handle);
                return Err(AsioError::ConfigError("MapViewOfFile failed".to_string()));
            }

            // Initialize header
            let header = ptr.Value as *mut SharedMemoryHeader;
            std::ptr::write(
                header,
                SharedMemoryHeader {
                    magic: SHARED_MEMORY_MAGIC,
                    version: SHARED_MEMORY_VERSION,
                    channels: config.input_channels.max(config.output_channels),
                    sample_rate: config.sample_rate,
                    buffer_size: config.buffer_size,
                    read_pos: 0,
                    write_pos: 0,
                    flags: flags::SERVICE_CONNECTED,
                    reserved: [0; 16],
                },
            );

            Ok(Self {
                handle,
                ptr: ptr.Value,
                size: total_size,
            })
        }
    }

    fn read_flags(&self) -> u64 {
        // Use addr_of! to get raw pointer without creating a reference (avoids UB with packed struct)
        // Then read_unaligned handles potentially unaligned access safely
        unsafe {
            let header_ptr = self.ptr.cast::<SharedMemoryHeader>();
            let flags_ptr = std::ptr::addr_of!((*header_ptr).flags);
            std::ptr::read_unaligned(flags_ptr)
        }
    }

    fn is_driver_connected(&self) -> bool {
        self.read_flags() & flags::DRIVER_RUNNING != 0
    }

    fn is_driver_running(&self) -> bool {
        self.read_flags() & flags::DRIVER_RUNNING != 0
    }
}

#[cfg(target_os = "windows")]
impl Drop for SharedMemoryRegion {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Memory::UnmapViewOfFile;

        unsafe {
            if !self.ptr.is_null() {
                let _ =
                    UnmapViewOfFile(windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: self.ptr,
                    });
            }
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe impl Send for SharedMemoryRegion {}
#[cfg(target_os = "windows")]
unsafe impl Sync for SharedMemoryRegion {}

// Stub implementation for non-Windows
#[cfg(not(target_os = "windows"))]
pub struct VirtualDevice {
    config: VirtualDeviceConfig,
}

#[cfg(not(target_os = "windows"))]
impl VirtualDevice {
    /// Create a new virtual ASIO device (stub on non-Windows).
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(_config: VirtualDeviceConfig) -> AsioResult<Self> {
        Err(AsioError::NotAvailable)
    }

    /// Get the device configuration.
    #[must_use]
    pub fn config(&self) -> &VirtualDeviceConfig {
        &self.config
    }

    /// Check if an ASIO driver is connected.
    #[must_use]
    pub fn is_driver_connected(&self) -> bool {
        false
    }

    /// Check if the driver is actively running.
    #[must_use]
    pub fn is_driver_running(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = VirtualDeviceConfig::default();
        assert_eq!(config.name, "AudioMatrix Virtual");
        assert_eq!(config.input_channels, 2);
        assert_eq!(config.output_channels, 2);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.buffer_size, 256);
    }

    #[test]
    fn shared_memory_header_size() {
        assert_eq!(std::mem::size_of::<SharedMemoryHeader>(), 64);
    }

    #[test]
    fn flags_values() {
        assert_eq!(flags::SERVICE_CONNECTED, 1);
        assert_eq!(flags::DRIVER_RUNNING, 2);
        assert_eq!(flags::BUFFER_SWITCH_REQ, 4);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn virtual_device_not_available_on_non_windows() {
        let config = VirtualDeviceConfig::default();
        let result = VirtualDevice::new(config);
        assert!(matches!(result, Err(AsioError::NotAvailable)));
    }
}
