//! ASIO audio streaming.
//!
//! Provides real-time audio streaming from/to ASIO devices.

#[cfg(all(target_os = "windows", feature = "asio"))]
use crate::device::AsioDevice;
#[cfg(not(all(target_os = "windows", feature = "asio")))]
use crate::error::AsioError;
use crate::error::AsioResult;

#[cfg(all(target_os = "windows", feature = "asio"))]
use cpal::traits::{DeviceTrait, StreamTrait};

/// Configuration for an ASIO audio stream.
#[derive(Debug, Clone)]
pub struct AsioStreamConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Buffer size in samples per channel.
    pub buffer_size: u32,
    /// Number of channels.
    pub channels: u16,
}

impl Default for AsioStreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 256,
            channels: 2,
        }
    }
}

/// Stream state for tracking lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// Stream is created but not started.
    Stopped,
    /// Stream is actively processing audio.
    Running,
    /// Stream encountered an error.
    Error,
}

/// An active ASIO audio stream.
///
/// This represents a running audio stream that processes samples
/// through a user-provided callback. The stream runs on a high-priority
/// audio thread managed by the ASIO driver.
///
/// # Real-time Safety
///
/// The audio callback provided to this stream MUST be lock-free:
/// - No heap allocations
/// - No mutex/rwlock acquisition
/// - No system calls
/// - No I/O operations
///
/// Violating these constraints will cause audio glitches.
pub struct AsioStream {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    inner: cpal::Stream,
    config: AsioStreamConfig,
    state: std::sync::atomic::AtomicU8,
}

impl AsioStream {
    /// Create and start an input stream.
    ///
    /// The callback receives audio samples from the ASIO device.
    /// Samples are interleaved if multi-channel.
    ///
    /// # Arguments
    ///
    /// * `device` - The ASIO device to stream from
    /// * `config` - Stream configuration
    /// * `callback` - Called for each buffer of input samples
    /// * `error_callback` - Called if a stream error occurs
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn build_input<D, E>(
        device: &AsioDevice,
        config: AsioStreamConfig,
        mut callback: D,
        mut error_callback: E,
    ) -> AsioResult<Self>
    where
        D: FnMut(&[f32]) + Send + 'static,
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let stream_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
        };

        let stream = device.cpal_device().build_input_stream(
            &stream_config,
            move |data: &[f32], _info| {
                callback(data);
            },
            move |err| {
                error_callback(err);
            },
            None,
        )?;

        Ok(Self {
            inner: stream,
            config,
            state: std::sync::atomic::AtomicU8::new(StreamState::Stopped as u8),
        })
    }

    /// Create and start an output stream.
    ///
    /// The callback fills the buffer with audio samples to send to the device.
    /// Samples should be interleaved if multi-channel.
    ///
    /// # Arguments
    ///
    /// * `device` - The ASIO device to stream to
    /// * `config` - Stream configuration
    /// * `callback` - Called to fill each output buffer
    /// * `error_callback` - Called if a stream error occurs
    #[cfg(all(target_os = "windows", feature = "asio"))]
    pub fn build_output<D, E>(
        device: &AsioDevice,
        config: AsioStreamConfig,
        mut callback: D,
        mut error_callback: E,
    ) -> AsioResult<Self>
    where
        D: FnMut(&mut [f32]) + Send + 'static,
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let stream_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
        };

        let stream = device.cpal_device().build_output_stream(
            &stream_config,
            move |data: &mut [f32], _info| {
                callback(data);
            },
            move |err| {
                error_callback(err);
            },
            None,
        )?;

        Ok(Self {
            inner: stream,
            config,
            state: std::sync::atomic::AtomicU8::new(StreamState::Stopped as u8),
        })
    }

    /// Start the audio stream.
    ///
    /// After calling this, the audio callback will begin receiving/sending samples.
    pub fn play(&self) -> AsioResult<()> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            self.inner.play()?;
            self.state.store(
                StreamState::Running as u8,
                std::sync::atomic::Ordering::Release,
            );
            tracing::debug!("ASIO stream started");
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Pause the audio stream.
    ///
    /// The stream can be resumed with `play()`.
    pub fn pause(&self) -> AsioResult<()> {
        #[cfg(all(target_os = "windows", feature = "asio"))]
        {
            self.inner.pause()?;
            self.state.store(
                StreamState::Stopped as u8,
                std::sync::atomic::Ordering::Release,
            );
            tracing::debug!("ASIO stream paused");
            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "asio")))]
        {
            Err(AsioError::NotAvailable)
        }
    }

    /// Get the current stream state.
    #[must_use]
    pub fn state(&self) -> StreamState {
        match self.state.load(std::sync::atomic::Ordering::Acquire) {
            0 => StreamState::Stopped,
            1 => StreamState::Running,
            _ => StreamState::Error,
        }
    }

    /// Get the stream configuration.
    #[must_use]
    pub fn config(&self) -> &AsioStreamConfig {
        &self.config
    }

    /// Check if the stream is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state() == StreamState::Running
    }
}

impl std::fmt::Debug for AsioStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsioStream")
            .field("config", &self.config)
            .field("state", &self.state())
            .finish()
    }
}

/// Builder for creating ASIO streams with complex configurations.
#[derive(Debug, Clone)]
pub struct AsioStreamBuilder {
    sample_rate: u32,
    buffer_size: u32,
    channels: u16,
}

impl AsioStreamBuilder {
    /// Create a new stream builder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: 256,
            channels: 2,
        }
    }

    /// Set the sample rate.
    #[must_use]
    pub fn sample_rate(mut self, rate: u32) -> Self {
        self.sample_rate = rate;
        self
    }

    /// Set the buffer size in samples per channel.
    #[must_use]
    pub fn buffer_size(mut self, size: u32) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set the number of channels.
    #[must_use]
    pub fn channels(mut self, count: u16) -> Self {
        self.channels = count;
        self
    }

    /// Build the stream configuration.
    #[must_use]
    pub fn build(self) -> AsioStreamConfig {
        AsioStreamConfig {
            sample_rate: self.sample_rate,
            buffer_size: self.buffer_size,
            channels: self.channels,
        }
    }
}

impl Default for AsioStreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_config_default() {
        let config = AsioStreamConfig::default();
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.buffer_size, 256);
        assert_eq!(config.channels, 2);
    }

    #[test]
    fn stream_builder() {
        let config = AsioStreamBuilder::new()
            .sample_rate(96000)
            .buffer_size(128)
            .channels(8)
            .build();

        assert_eq!(config.sample_rate, 96000);
        assert_eq!(config.buffer_size, 128);
        assert_eq!(config.channels, 8);
    }

    #[test]
    fn stream_state_values() {
        assert_eq!(StreamState::Stopped as u8, 0);
        assert_eq!(StreamState::Running as u8, 1);
        assert_eq!(StreamState::Error as u8, 2);
    }
}
