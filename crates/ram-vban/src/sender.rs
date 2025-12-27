//! VBAN stream sender.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::net::UdpSocket;

use crate::protocol::{VbanHeader, VbanProtocol, VbanSampleRate, VbanSubProtocol};
use crate::{Result, HEADER_SIZE};

/// Configuration for a VBAN sender.
#[derive(Debug, Clone)]
pub struct VbanSenderConfig {
    /// Stream name (max 16 characters).
    pub stream_name: String,
    /// Sample rate.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u8,
    /// Destination address.
    pub destination: SocketAddr,
}

/// Sends VBAN audio streams over UDP.
pub struct VbanSender {
    socket: Arc<UdpSocket>,
    config: VbanSenderConfig,
    frame_counter: AtomicU32,
    sample_rate_code: VbanSampleRate,
}

impl VbanSender {
    /// Creates a new VBAN sender.
    ///
    /// # Errors
    ///
    /// Returns an error if socket binding fails or sample rate is unsupported.
    pub async fn new(config: VbanSenderConfig) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let sample_rate_code = VbanSampleRate::from_hz(config.sample_rate)?;

        Ok(Self {
            socket: Arc::new(socket),
            config,
            frame_counter: AtomicU32::new(0),
            sample_rate_code,
        })
    }

    /// Sends audio samples as VBAN packets.
    ///
    /// The samples should be interleaved if multi-channel.
    ///
    /// # Errors
    ///
    /// Returns an error if sending fails.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn send(&self, samples: &[f32]) -> Result<()> {
        let channels = self.config.channels as usize;
        let samples_per_frame = samples.len() / channels;

        if samples_per_frame == 0 || samples_per_frame > 256 {
            return Ok(()); // Invalid frame size
        }

        let header = VbanHeader {
            sub_protocol: VbanSubProtocol::Audio,
            sample_rate: self.sample_rate_code,
            samples_per_frame: (samples_per_frame - 1) as u8,
            channels: (channels - 1) as u8,
            format: VbanProtocol::Float32,
            stream_name: self.config.stream_name.clone(),
            frame_counter: self.frame_counter.fetch_add(1, Ordering::Relaxed),
        };

        let header_bytes = header.to_bytes();

        // Convert f32 samples to bytes
        let audio_bytes: Vec<u8> = samples.iter().flat_map(|&s| s.to_le_bytes()).collect();

        // Combine header and audio
        let mut packet = Vec::with_capacity(HEADER_SIZE + audio_bytes.len());
        packet.extend_from_slice(&header_bytes);
        packet.extend_from_slice(&audio_bytes);

        self.socket
            .send_to(&packet, self.config.destination)
            .await?;
        Ok(())
    }

    /// Returns the stream name.
    #[must_use]
    pub fn stream_name(&self) -> &str {
        &self.config.stream_name
    }

    /// Returns the destination address.
    #[must_use]
    pub fn destination(&self) -> SocketAddr {
        self.config.destination
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sender_creates_successfully() {
        let config = VbanSenderConfig {
            stream_name: "Test".into(),
            sample_rate: 48000,
            channels: 2,
            destination: "127.0.0.1:6980".parse().unwrap(),
        };

        let sender = VbanSender::new(config).await.unwrap();
        assert_eq!(sender.stream_name(), "Test");
    }
}
