//! VBAN protocol definitions and header parsing.

use crate::{Error, Result, HEADER_SIZE};

/// VBAN magic bytes: "VBAN"
const VBAN_MAGIC: [u8; 4] = [b'V', b'B', b'A', b'N'];

/// VBAN sub-protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VbanSubProtocol {
    /// Audio data.
    Audio = 0x00,
    /// Serial data (MIDI, etc.).
    Serial = 0x20,
    /// Text data.
    Text = 0x40,
    /// Service data (ping, etc.).
    Service = 0x60,
}

impl TryFrom<u8> for VbanSubProtocol {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value & 0xE0 {
            0x00 => Ok(Self::Audio),
            0x20 => Ok(Self::Serial),
            0x40 => Ok(Self::Text),
            0x60 => Ok(Self::Service),
            _ => Err(Error::InvalidHeader(format!(
                "unknown sub-protocol: 0x{value:02X}"
            ))),
        }
    }
}

/// VBAN sample rate codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VbanSampleRate {
    /// 6000 Hz
    Hz6000 = 0,
    /// 12000 Hz
    Hz12000 = 1,
    /// 24000 Hz
    Hz24000 = 2,
    /// 48000 Hz
    Hz48000 = 3,
    /// 96000 Hz
    Hz96000 = 4,
    /// 192000 Hz
    Hz192000 = 5,
    /// 384000 Hz
    Hz384000 = 6,
    /// 8000 Hz
    Hz8000 = 7,
    /// 16000 Hz
    Hz16000 = 8,
    /// 32000 Hz
    Hz32000 = 9,
    /// 64000 Hz
    Hz64000 = 10,
    /// 128000 Hz
    Hz128000 = 11,
    /// 256000 Hz
    Hz256000 = 12,
    /// 512000 Hz
    Hz512000 = 13,
    /// 11025 Hz
    Hz11025 = 14,
    /// 22050 Hz
    Hz22050 = 15,
    /// 44100 Hz
    Hz44100 = 16,
    /// 88200 Hz
    Hz88200 = 17,
    /// 176400 Hz
    Hz176400 = 18,
    /// 352800 Hz
    Hz352800 = 19,
    /// 705600 Hz
    Hz705600 = 20,
}

impl VbanSampleRate {
    /// Converts the enum to the actual sample rate in Hz.
    #[must_use]
    #[allow(clippy::unreadable_literal)]
    pub const fn to_hz(self) -> u32 {
        match self {
            Self::Hz6000 => 6000,
            Self::Hz12000 => 12000,
            Self::Hz24000 => 24000,
            Self::Hz48000 => 48000,
            Self::Hz96000 => 96000,
            Self::Hz192000 => 192000,
            Self::Hz384000 => 384000,
            Self::Hz8000 => 8000,
            Self::Hz16000 => 16000,
            Self::Hz32000 => 32000,
            Self::Hz64000 => 64000,
            Self::Hz128000 => 128000,
            Self::Hz256000 => 256000,
            Self::Hz512000 => 512000,
            Self::Hz11025 => 11025,
            Self::Hz22050 => 22050,
            Self::Hz44100 => 44100,
            Self::Hz88200 => 88200,
            Self::Hz176400 => 176400,
            Self::Hz352800 => 352800,
            Self::Hz705600 => 705600,
        }
    }

    /// Creates a sample rate enum from Hz value.
    ///
    /// # Errors
    ///
    /// Returns an error if the sample rate is not supported.
    #[allow(clippy::unreadable_literal)]
    pub fn from_hz(hz: u32) -> Result<Self> {
        match hz {
            6000 => Ok(Self::Hz6000),
            12000 => Ok(Self::Hz12000),
            24000 => Ok(Self::Hz24000),
            48000 => Ok(Self::Hz48000),
            96000 => Ok(Self::Hz96000),
            192000 => Ok(Self::Hz192000),
            384000 => Ok(Self::Hz384000),
            8000 => Ok(Self::Hz8000),
            16000 => Ok(Self::Hz16000),
            32000 => Ok(Self::Hz32000),
            64000 => Ok(Self::Hz64000),
            128000 => Ok(Self::Hz128000),
            256000 => Ok(Self::Hz256000),
            512000 => Ok(Self::Hz512000),
            11025 => Ok(Self::Hz11025),
            22050 => Ok(Self::Hz22050),
            44100 => Ok(Self::Hz44100),
            88200 => Ok(Self::Hz88200),
            176400 => Ok(Self::Hz176400),
            352800 => Ok(Self::Hz352800),
            705600 => Ok(Self::Hz705600),
            _ => Err(Error::UnsupportedSampleRate(hz)),
        }
    }
}

impl TryFrom<u8> for VbanSampleRate {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value & 0x1F {
            0 => Ok(Self::Hz6000),
            1 => Ok(Self::Hz12000),
            2 => Ok(Self::Hz24000),
            3 => Ok(Self::Hz48000),
            4 => Ok(Self::Hz96000),
            5 => Ok(Self::Hz192000),
            6 => Ok(Self::Hz384000),
            7 => Ok(Self::Hz8000),
            8 => Ok(Self::Hz16000),
            9 => Ok(Self::Hz32000),
            10 => Ok(Self::Hz64000),
            11 => Ok(Self::Hz128000),
            12 => Ok(Self::Hz256000),
            13 => Ok(Self::Hz512000),
            14 => Ok(Self::Hz11025),
            15 => Ok(Self::Hz22050),
            16 => Ok(Self::Hz44100),
            17 => Ok(Self::Hz88200),
            18 => Ok(Self::Hz176400),
            19 => Ok(Self::Hz352800),
            20 => Ok(Self::Hz705600),
            n => Err(Error::InvalidHeader(format!(
                "unknown sample rate code: {n}"
            ))),
        }
    }
}

/// VBAN protocol data formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VbanProtocol {
    /// PCM signed 8-bit.
    Pcm8 = 0,
    /// PCM signed 16-bit.
    Pcm16 = 1,
    /// PCM signed 24-bit.
    Pcm24 = 2,
    /// PCM signed 32-bit.
    Pcm32 = 3,
    /// 32-bit float.
    Float32 = 4,
    /// 64-bit float.
    Float64 = 5,
    /// 12-bit packed.
    Pcm12 = 6,
    /// 10-bit packed.
    Pcm10 = 7,
}

impl VbanProtocol {
    /// Returns the size in bytes of a single sample.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn sample_size(self) -> usize {
        match self {
            Self::Pcm8 => 1,
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Pcm32 | Self::Float32 => 4,
            Self::Float64 => 8,
            Self::Pcm12 => 2, // Approximate (packed format)
            Self::Pcm10 => 2, // Approximate (packed format)
        }
    }
}

impl TryFrom<u8> for VbanProtocol {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value & 0x07 {
            0 => Ok(Self::Pcm8),
            1 => Ok(Self::Pcm16),
            2 => Ok(Self::Pcm24),
            3 => Ok(Self::Pcm32),
            4 => Ok(Self::Float32),
            5 => Ok(Self::Float64),
            6 => Ok(Self::Pcm12),
            7 => Ok(Self::Pcm10),
            _ => unreachable!(),
        }
    }
}

/// VBAN packet header (28 bytes).
#[derive(Debug, Clone)]
pub struct VbanHeader {
    /// Sub-protocol.
    pub sub_protocol: VbanSubProtocol,
    /// Sample rate.
    pub sample_rate: VbanSampleRate,
    /// Number of samples per frame minus 1.
    pub samples_per_frame: u8,
    /// Number of channels minus 1.
    pub channels: u8,
    /// Data format.
    pub format: VbanProtocol,
    /// Stream name (up to 16 characters).
    pub stream_name: String,
    /// Frame counter for sequence tracking.
    pub frame_counter: u32,
}

impl VbanHeader {
    /// Parses a VBAN header from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the header is invalid or too short.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(Error::InvalidHeader(format!(
                "header too short: {} bytes (need {})",
                data.len(),
                HEADER_SIZE
            )));
        }

        // Check magic
        if data[0..4] != VBAN_MAGIC {
            return Err(Error::InvalidHeader("invalid magic bytes".into()));
        }

        let sr_byte = data[4];
        let sub_protocol = VbanSubProtocol::try_from(sr_byte)?;
        let sample_rate = VbanSampleRate::try_from(sr_byte)?;
        let samples_per_frame = data[5];
        let channels = data[6];
        let format_byte = data[7];
        let format = VbanProtocol::try_from(format_byte)?;

        // Stream name (bytes 8-23, null-terminated)
        let name_bytes = &data[8..24];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(16);
        let stream_name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();

        // Frame counter (bytes 24-27, little-endian)
        let frame_counter = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);

        Ok(Self {
            sub_protocol,
            sample_rate,
            samples_per_frame,
            channels,
            format,
            stream_name,
            frame_counter,
        })
    }

    /// Serializes the header to bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0u8; HEADER_SIZE];

        // Magic
        bytes[0..4].copy_from_slice(&VBAN_MAGIC);

        // Sample rate + sub-protocol
        bytes[4] = (self.sub_protocol as u8) | (self.sample_rate as u8);

        // Samples per frame
        bytes[5] = self.samples_per_frame;

        // Channels
        bytes[6] = self.channels;

        // Format
        bytes[7] = self.format as u8;

        // Stream name
        let name_bytes = self.stream_name.as_bytes();
        let len = name_bytes.len().min(16);
        bytes[8..8 + len].copy_from_slice(&name_bytes[..len]);

        // Frame counter
        bytes[24..28].copy_from_slice(&self.frame_counter.to_le_bytes());

        bytes
    }

    /// Returns the actual number of samples per frame.
    #[must_use]
    pub const fn actual_samples(&self) -> usize {
        self.samples_per_frame as usize + 1
    }

    /// Returns the actual number of channels.
    #[must_use]
    pub const fn actual_channels(&self) -> usize {
        self.channels as usize + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rate_roundtrip() {
        let sr = VbanSampleRate::Hz48000;
        assert_eq!(sr.to_hz(), 48000);
        assert_eq!(VbanSampleRate::from_hz(48000).unwrap(), sr);
    }

    #[test]
    fn header_roundtrip() {
        let header = VbanHeader {
            sub_protocol: VbanSubProtocol::Audio,
            sample_rate: VbanSampleRate::Hz48000,
            samples_per_frame: 255,
            channels: 1,
            format: VbanProtocol::Float32,
            stream_name: "TestStream".into(),
            frame_counter: 12345,
        };

        let bytes = header.to_bytes();
        let parsed = VbanHeader::parse(&bytes).unwrap();

        assert_eq!(parsed.sub_protocol, header.sub_protocol);
        assert_eq!(parsed.sample_rate, header.sample_rate);
        assert_eq!(parsed.samples_per_frame, header.samples_per_frame);
        assert_eq!(parsed.channels, header.channels);
        assert_eq!(parsed.format, header.format);
        assert_eq!(parsed.stream_name, header.stream_name);
        assert_eq!(parsed.frame_counter, header.frame_counter);
    }
}
