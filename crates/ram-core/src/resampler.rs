//! Sample rate conversion using rubato.
//!
//! This module provides high-quality sample rate conversion for audio streams.
//! It wraps the `rubato` crate to provide a simple interface for resampling
//! between different sample rates.

use rubato::{FftFixedIn, FftFixedOut, Resampler as _};

use crate::{Error, Result, Sample};

/// Quality preset for resampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResamplerQuality {
    /// Fast resampling with lower quality. Suitable for preview/monitoring.
    Fast,
    /// Balanced quality and performance. Good for most uses.
    #[default]
    Normal,
    /// High quality resampling. Best for final output but uses more CPU.
    High,
}

impl ResamplerQuality {
    /// Returns the number of sinc interpolation points for this quality level.
    #[must_use]
    pub fn sinc_len(self) -> usize {
        match self {
            Self::Fast => 64,
            Self::Normal => 128,
            Self::High => 256,
        }
    }

    /// Returns the oversampling factor for this quality level.
    #[must_use]
    pub fn oversampling_factor(self) -> usize {
        match self {
            Self::Fast => 128,
            Self::Normal => 256,
            Self::High => 512,
        }
    }
}

/// Internal resampler type (either fixed-in or fixed-out).
enum ResamplerInner {
    FixedIn(FftFixedIn<Sample>),
    FixedOut(FftFixedOut<Sample>),
}

/// A sample rate converter.
///
/// This resampler converts audio from one sample rate to another with
/// configurable quality. It maintains internal state for continuous
/// streaming operation.
pub struct Resampler {
    /// The inner rubato resampler.
    inner: ResamplerInner,
    /// Number of channels.
    channels: usize,
    /// Input sample rate.
    input_rate: u32,
    /// Output sample rate.
    output_rate: u32,
}

impl Resampler {
    /// Creates a new resampler.
    ///
    /// # Arguments
    ///
    /// * `input_rate` - Input sample rate in Hz
    /// * `output_rate` - Output sample rate in Hz
    /// * `channels` - Number of audio channels
    /// * `chunk_size` - Processing chunk size (frames)
    /// * `quality` - Resampling quality preset
    ///
    /// # Errors
    ///
    /// Returns an error if the resampler cannot be created with the given parameters.
    pub fn new(
        input_rate: u32,
        output_rate: u32,
        channels: usize,
        chunk_size: usize,
        quality: ResamplerQuality,
    ) -> Result<Self> {
        if channels == 0 {
            return Err(Error::InvalidChannelCount(0));
        }

        // Determine whether to use fixed-in or fixed-out based on rate ratio
        let inner = if input_rate <= output_rate {
            // Upsampling or unity: use fixed input
            ResamplerInner::FixedIn(
                FftFixedIn::<Sample>::new(
                    input_rate as usize,
                    output_rate as usize,
                    chunk_size,
                    quality.oversampling_factor(),
                    channels,
                )
                .map_err(|e| Error::Resampler(e.to_string()))?,
            )
        } else {
            // Downsampling: use fixed output for efficiency
            ResamplerInner::FixedOut(
                FftFixedOut::<Sample>::new(
                    input_rate as usize,
                    output_rate as usize,
                    chunk_size,
                    quality.oversampling_factor(),
                    channels,
                )
                .map_err(|e| Error::Resampler(e.to_string()))?,
            )
        };

        Ok(Self {
            inner,
            channels,
            input_rate,
            output_rate,
        })
    }

    /// Creates a resampler with default quality.
    ///
    /// # Errors
    ///
    /// Returns an error if the resampler cannot be created.
    pub fn new_default(
        input_rate: u32,
        output_rate: u32,
        channels: usize,
        chunk_size: usize,
    ) -> Result<Self> {
        Self::new(
            input_rate,
            output_rate,
            channels,
            chunk_size,
            ResamplerQuality::default(),
        )
    }

    /// Returns the input sample rate.
    #[must_use]
    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Returns the output sample rate.
    #[must_use]
    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Returns the number of input frames needed for the next process call.
    #[must_use]
    pub fn input_frames_next(&self) -> usize {
        match &self.inner {
            ResamplerInner::FixedIn(r) => r.input_frames_next(),
            ResamplerInner::FixedOut(r) => r.input_frames_next(),
        }
    }

    /// Returns the number of output frames that will be produced.
    #[must_use]
    pub fn output_frames_next(&self) -> usize {
        match &self.inner {
            ResamplerInner::FixedIn(r) => r.output_frames_next(),
            ResamplerInner::FixedOut(r) => r.output_frames_next(),
        }
    }

    /// Returns the resampling ratio (`output_rate` / `input_rate`).
    #[must_use]
    pub fn ratio(&self) -> f64 {
        f64::from(self.output_rate) / f64::from(self.input_rate)
    }

    /// Returns true if this is an identity resampler (same input/output rate).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.input_rate == self.output_rate
    }

    /// Processes a chunk of audio.
    ///
    /// # Arguments
    ///
    /// * `input` - Input samples, one vector per channel
    /// * `output` - Output buffer, one vector per channel
    ///
    /// # Returns
    ///
    /// The number of output frames produced.
    ///
    /// # Errors
    ///
    /// Returns an error if processing fails.
    pub fn process(&mut self, input: &[Vec<Sample>], output: &mut [Vec<Sample>]) -> Result<usize> {
        if input.len() != self.channels || output.len() != self.channels {
            return Err(Error::InvalidChannelCount(input.len()));
        }

        let (_, out_frames) = match &mut self.inner {
            ResamplerInner::FixedIn(r) => r
                .process_into_buffer(input, output, None)
                .map_err(|e| Error::Resampler(e.to_string()))?,
            ResamplerInner::FixedOut(r) => r
                .process_into_buffer(input, output, None)
                .map_err(|e| Error::Resampler(e.to_string()))?,
        };

        Ok(out_frames)
    }

    /// Resets the resampler state.
    ///
    /// Call this when starting a new stream or after a discontinuity.
    pub fn reset(&mut self) {
        match &mut self.inner {
            ResamplerInner::FixedIn(r) => r.reset(),
            ResamplerInner::FixedOut(r) => r.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_quality_sinc_len() {
        assert!(ResamplerQuality::Fast.sinc_len() < ResamplerQuality::Normal.sinc_len());
        assert!(ResamplerQuality::Normal.sinc_len() < ResamplerQuality::High.sinc_len());
    }

    #[test]
    fn resampler_quality_default() {
        assert_eq!(ResamplerQuality::default(), ResamplerQuality::Normal);
    }

    #[test]
    fn resampler_new_upsampling() {
        let resampler = Resampler::new(44100, 48000, 2, 256, ResamplerQuality::Normal);
        assert!(resampler.is_ok());
        let r = resampler.unwrap();
        assert_eq!(r.input_rate(), 44100);
        assert_eq!(r.output_rate(), 48000);
        assert_eq!(r.channels(), 2);
        assert!(!r.is_identity());
    }

    #[test]
    fn resampler_new_downsampling() {
        let resampler = Resampler::new(48000, 44100, 2, 256, ResamplerQuality::Normal);
        assert!(resampler.is_ok());
    }

    #[test]
    fn resampler_identity() {
        let resampler = Resampler::new(48000, 48000, 2, 256, ResamplerQuality::Fast);
        assert!(resampler.is_ok());
        let r = resampler.unwrap();
        assert!(r.is_identity());
        assert!((r.ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn resampler_zero_channels_fails() {
        let result = Resampler::new(44100, 48000, 0, 256, ResamplerQuality::Normal);
        assert!(result.is_err());
    }

    #[test]
    fn resampler_ratio() {
        let r = Resampler::new(44100, 48000, 1, 256, ResamplerQuality::Normal).unwrap();
        let expected = 48000.0 / 44100.0;
        assert!((r.ratio() - expected).abs() < 0.0001);
    }

    #[test]
    fn resampler_process_basic() {
        let mut r = Resampler::new(44100, 48000, 1, 256, ResamplerQuality::Fast).unwrap();

        let input_frames = r.input_frames_next();
        let output_frames = r.output_frames_next();

        let input = vec![vec![0.5; input_frames]];
        let mut output = vec![vec![0.0; output_frames]];

        let result = r.process(&input, &mut output);
        assert!(result.is_ok());
    }

    #[test]
    fn resampler_reset() {
        let mut r = Resampler::new(44100, 48000, 1, 256, ResamplerQuality::Normal).unwrap();
        r.reset(); // Should not panic
    }

    #[test]
    fn resampler_default_constructor() {
        let r = Resampler::new_default(44100, 48000, 2, 256);
        assert!(r.is_ok());
    }

    #[test]
    fn resampler_frames_accessors() {
        let r = Resampler::new(44100, 48000, 1, 256, ResamplerQuality::Normal).unwrap();
        assert!(r.input_frames_next() > 0);
        assert!(r.output_frames_next() > 0);
    }
}
