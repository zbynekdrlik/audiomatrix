//! Error types for ASIO operations.

use thiserror::Error;

/// Result type for ASIO operations.
pub type AsioResult<T> = Result<T, AsioError>;

/// Errors that can occur during ASIO operations.
#[derive(Debug, Error)]
pub enum AsioError {
    /// ASIO is not available on this platform.
    #[error("ASIO is not available (Windows only with 'asio' feature)")]
    NotAvailable,

    /// Failed to initialize ASIO host.
    #[error("failed to initialize ASIO host: {0}")]
    HostInitFailed(String),

    /// Device not found.
    #[error("ASIO device not found: {0}")]
    DeviceNotFound(String),

    /// Failed to open device.
    #[error("failed to open ASIO device '{name}': {reason}")]
    DeviceOpenFailed { name: String, reason: String },

    /// Failed to start stream.
    #[error("failed to start ASIO stream: {0}")]
    StreamStartFailed(String),

    /// Stream error during operation.
    #[error("ASIO stream error: {0}")]
    StreamError(String),

    /// Configuration error.
    #[error("invalid ASIO configuration: {0}")]
    ConfigError(String),

    /// Buffer size not supported.
    #[error("buffer size {requested} not supported (min: {min}, max: {max})")]
    BufferSizeNotSupported { requested: u32, min: u32, max: u32 },

    /// Sample rate not supported.
    #[error("sample rate {0} Hz not supported by device")]
    SampleRateNotSupported(u32),

    /// Channel count exceeded.
    #[error("channel count {requested} exceeds device maximum {max}")]
    ChannelCountExceeded { requested: u16, max: u16 },

    /// Device is busy (in use by another application).
    #[error("ASIO device '{0}' is busy (in use by another application)")]
    DeviceBusy(String),

    /// Underlying cpal error.
    #[error("cpal error: {0}")]
    CpalError(String),
}

impl From<cpal::HostUnavailable> for AsioError {
    fn from(e: cpal::HostUnavailable) -> Self {
        Self::HostInitFailed(e.to_string())
    }
}

impl From<cpal::DevicesError> for AsioError {
    fn from(e: cpal::DevicesError) -> Self {
        Self::CpalError(format!("devices error: {e}"))
    }
}

impl From<cpal::DeviceNameError> for AsioError {
    fn from(e: cpal::DeviceNameError) -> Self {
        Self::CpalError(format!("device name error: {e}"))
    }
}

impl From<cpal::SupportedStreamConfigsError> for AsioError {
    fn from(e: cpal::SupportedStreamConfigsError) -> Self {
        Self::CpalError(format!("stream config error: {e}"))
    }
}

impl From<cpal::BuildStreamError> for AsioError {
    fn from(e: cpal::BuildStreamError) -> Self {
        Self::StreamStartFailed(e.to_string())
    }
}

impl From<cpal::PlayStreamError> for AsioError {
    fn from(e: cpal::PlayStreamError) -> Self {
        Self::StreamError(format!("play error: {e}"))
    }
}

impl From<cpal::PauseStreamError> for AsioError {
    fn from(e: cpal::PauseStreamError) -> Self {
        Self::StreamError(format!("pause error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_not_available() {
        let err = AsioError::NotAvailable;
        assert_eq!(
            err.to_string(),
            "ASIO is not available (Windows only with 'asio' feature)"
        );
    }

    #[test]
    fn error_display_host_init_failed() {
        let err = AsioError::HostInitFailed("test error".to_string());
        assert_eq!(
            err.to_string(),
            "failed to initialize ASIO host: test error"
        );
    }

    #[test]
    fn error_display_device_not_found() {
        let err = AsioError::DeviceNotFound("MyDevice".to_string());
        assert_eq!(err.to_string(), "ASIO device not found: MyDevice");
    }

    #[test]
    fn error_display_device_open_failed() {
        let err = AsioError::DeviceOpenFailed {
            name: "MyDevice".to_string(),
            reason: "busy".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "failed to open ASIO device 'MyDevice': busy"
        );
    }

    #[test]
    fn error_display_stream_start_failed() {
        let err = AsioError::StreamStartFailed("timeout".to_string());
        assert_eq!(err.to_string(), "failed to start ASIO stream: timeout");
    }

    #[test]
    fn error_display_stream_error() {
        let err = AsioError::StreamError("underrun".to_string());
        assert_eq!(err.to_string(), "ASIO stream error: underrun");
    }

    #[test]
    fn error_display_config_error() {
        let err = AsioError::ConfigError("invalid rate".to_string());
        assert_eq!(err.to_string(), "invalid ASIO configuration: invalid rate");
    }

    #[test]
    fn error_display_buffer_size_not_supported() {
        let err = AsioError::BufferSizeNotSupported {
            requested: 32,
            min: 64,
            max: 4096,
        };
        assert_eq!(
            err.to_string(),
            "buffer size 32 not supported (min: 64, max: 4096)"
        );
    }

    #[test]
    fn error_display_sample_rate_not_supported() {
        let err = AsioError::SampleRateNotSupported(22050);
        assert_eq!(
            err.to_string(),
            "sample rate 22050 Hz not supported by device"
        );
    }

    #[test]
    fn error_display_channel_count_exceeded() {
        let err = AsioError::ChannelCountExceeded {
            requested: 64,
            max: 32,
        };
        assert_eq!(
            err.to_string(),
            "channel count 64 exceeds device maximum 32"
        );
    }

    #[test]
    fn error_display_device_busy() {
        let err = AsioError::DeviceBusy("Dante".to_string());
        assert_eq!(
            err.to_string(),
            "ASIO device 'Dante' is busy (in use by another application)"
        );
    }

    #[test]
    fn error_display_cpal_error() {
        let err = AsioError::CpalError("unknown".to_string());
        assert_eq!(err.to_string(), "cpal error: unknown");
    }

    #[test]
    fn error_debug_impl() {
        let err = AsioError::NotAvailable;
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotAvailable"));
    }
}
