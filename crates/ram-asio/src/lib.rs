//! ASIO device support for AudioMatrix.
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

pub mod host;
pub mod device;
pub mod stream;
pub mod error;

pub use host::AsioHost;
pub use device::{AsioDevice, AsioDeviceInfo};
pub use stream::AsioStream;
pub use error::{AsioError, AsioResult};

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
