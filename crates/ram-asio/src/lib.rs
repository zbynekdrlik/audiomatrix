//! ASIO device support for `AudioMatrix`.
//!
// Clippy pedantic lints relaxed for conditional compilation:
// - unused_self: methods use self on Windows with ASIO feature but not on other platforms
// - missing_errors_doc: error conditions are platform-dependent
#![allow(clippy::unused_self, clippy::missing_errors_doc)]
//!
//! This crate provides Windows ASIO driver integration:
//! - Hardware ASIO device enumeration and connection
//! - Virtual ASIO device creation for DAW integration
//! - Real-time audio streaming with lock-free buffers
//!
//! # Feature Flags
//!
//! - `asio` - Enable ASIO support (Windows only, requires ASIO SDK)
//!
//! # Requirements
//!
//! To build with ASIO support:
//! 1. Install LLVM/Clang and set `LIBCLANG_PATH`
//! 2. Either set `CPAL_ASIO_DIR` to ASIO SDK path, or let it auto-download
//! 3. Build with `--features asio`
//!
//! # Example
//!
//! ```ignore
//! use ram_asio::AsioHost;
//!
//! // Get the ASIO host (Windows only with asio feature)
//! let host = AsioHost::new()?;
//!
//! // List all ASIO devices
//! for device in host.devices() {
//!     println!("ASIO Device: {} ({} channels)", device.name(), device.channels());
//! }
//! ```

pub mod device;
pub mod error;
pub mod host;
pub mod stream;

pub use device::{AsioDevice, AsioDeviceInfo};
pub use error::{AsioError, AsioResult};
pub use host::AsioHost;
pub use stream::AsioStream;

/// Check if ASIO support is available at runtime.
///
/// Returns `true` if:
/// - Running on Windows
/// - Built with the `asio` feature
/// - ASIO host can be initialized
#[must_use]
pub fn is_asio_available() -> bool {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        AsioHost::new().is_ok()
    }
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    {
        false
    }
}

/// Get the ASIO host if available.
///
/// Returns `None` if:
/// - Not running on Windows
/// - Not built with `asio` feature
/// - ASIO initialization fails
#[must_use]
pub fn get_asio_host() -> Option<AsioHost> {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        AsioHost::new().ok()
    }
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    fn is_asio_available_returns_false_on_non_windows() {
        assert!(!is_asio_available());
    }

    #[test]
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    fn get_asio_host_returns_none_on_non_windows() {
        assert!(get_asio_host().is_none());
    }

    #[test]
    fn re_exports_are_accessible() {
        // Verify re-exports work correctly
        let _: fn() -> error::AsioResult<host::AsioHost> = host::AsioHost::new;
        let config = device::AsioDeviceConfig::default();
        assert_eq!(config.sample_rate, 48000);
    }
}
