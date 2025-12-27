//! VBAN stream receiver.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::net::UdpSocket;
use tracing::{debug, trace};

use crate::protocol::VbanHeader;
use crate::{Error, Result, DEFAULT_PORT, HEADER_SIZE, MAX_PACKET_SIZE};

/// Information about a discovered VBAN stream.
#[derive(Debug, Clone)]
pub struct VbanStreamInfo {
    /// Stream name.
    pub name: String,
    /// Source address.
    pub source: SocketAddr,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u8,
    /// Last seen frame counter.
    pub last_frame: u32,
}

/// Receives VBAN audio streams over UDP.
pub struct VbanReceiver {
    socket: Arc<UdpSocket>,
    streams: Arc<RwLock<HashMap<String, VbanStreamInfo>>>,
}

impl VbanReceiver {
    /// Creates a new VBAN receiver bound to the specified port.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound.
    pub async fn bind(port: u16) -> Result<Self> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{port}")).await?;
        debug!("VBAN receiver bound to port {port}");

        Ok(Self {
            socket: Arc::new(socket),
            streams: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Creates a receiver on the default VBAN port.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket cannot be bound.
    pub async fn new() -> Result<Self> {
        Self::bind(DEFAULT_PORT).await
    }

    /// Receives a single VBAN packet.
    ///
    /// Returns the header and audio samples (as f32).
    ///
    /// # Errors
    ///
    /// Returns an error if the packet is invalid or reception fails.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn receive(&self) -> Result<(VbanHeader, Vec<f32>, SocketAddr)> {
        let mut buf = [0u8; MAX_PACKET_SIZE];

        let (len, addr) = self.socket.recv_from(&mut buf).await?;
        trace!("Received {len} bytes from {addr}");

        if len < HEADER_SIZE {
            return Err(Error::InvalidHeader(format!(
                "packet too short: {len} bytes"
            )));
        }

        let header = VbanHeader::parse(&buf[..len])?;

        // Update stream info
        {
            let mut streams = self.streams.write();
            streams.insert(
                header.stream_name.clone(),
                VbanStreamInfo {
                    name: header.stream_name.clone(),
                    source: addr,
                    sample_rate: header.sample_rate.to_hz(),
                    channels: header.actual_channels() as u8,
                    last_frame: header.frame_counter,
                },
            );
        }

        // Parse audio data (assuming float32 for now)
        let audio_data = &buf[HEADER_SIZE..len];
        let samples: Vec<f32> = audio_data
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        Ok((header, samples, addr))
    }

    /// Returns information about all discovered streams.
    #[must_use]
    pub fn streams(&self) -> Vec<VbanStreamInfo> {
        self.streams.read().values().cloned().collect()
    }

    /// Returns information about a specific stream.
    #[must_use]
    pub fn stream(&self, name: &str) -> Option<VbanStreamInfo> {
        self.streams.read().get(name).cloned()
    }

    /// Returns the local address this receiver is bound to.
    ///
    /// # Errors
    ///
    /// Returns an error if the local address cannot be retrieved.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn receiver_binds_successfully() {
        // Use port 0 to get a random available port
        let receiver = VbanReceiver::bind(0).await.unwrap();
        assert!(receiver.local_addr().is_ok());
    }

    #[tokio::test]
    async fn streams_initially_empty() {
        let receiver = VbanReceiver::bind(0).await.unwrap();
        assert!(receiver.streams().is_empty());
    }
}
