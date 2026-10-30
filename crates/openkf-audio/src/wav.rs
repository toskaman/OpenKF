//! RIFF WAVE audio parser for USound assets extracted from Unreal Engine 2 `.uax` packages.
//!
//! Unreal Engine 2 stores sound effects in `.uax` packages as raw RIFF/WAVE PCM chunks.
//! This module provides decoding for 8-bit, 16-bit (Killing Floor standard), 24-bit, and 32-bit float PCM,
//! with native support for mono and stereo at any sample rate (22.05 kHz, 44.1 kHz, etc.).

use thiserror::Error;

/// Errors that can occur during RIFF WAV parsing.
#[derive(Debug, Error, PartialEq)]
pub enum WavError {
    #[error("Data too short to contain RIFF header (minimum 12 bytes required)")]
    DataTooShort,
    #[error("Invalid RIFF magic bytes, expected 'RIFF'")]
    InvalidRiffHeader,
    #[error("Invalid WAVE format bytes, expected 'WAVE'")]
    InvalidWaveFormat,
    #[error("Missing mandatory 'fmt ' chunk")]
    MissingFmtChunk,
    #[error("Missing mandatory 'data' chunk")]
    MissingDataChunk,
    #[error("Unsupported audio format tag: {0:#x} (only PCM=0x0001, IEEE_FLOAT=0x0003, EXTENSIBLE=0xFFFE are supported)")]
    UnsupportedFormat(u16),
    #[error("Unsupported bit depth: {0} bits (supported: 8, 16, 24, 32)")]
    UnsupportedBitDepth(u16),
    #[error("Unsupported channel count: {0} (supported: 1 to 8 channels)")]
    UnsupportedChannelCount(u16),
    #[error("Malformed chunk structure or corrupted payload")]
    CorruptedChunk,
    #[error("Sample data buffer length mismatch: expected {expected} bytes, got {actual}")]
    SampleDataLengthMismatch { expected: usize, actual: usize },
}

/// Decoded sound asset suitable for spatial playback and software mixing.
#[derive(Debug, Clone, PartialEq)]
pub struct WavSound {
    /// Number of audio channels (1 = Mono, 2 = Stereo).
    pub channels: u16,
    /// Sampling frequency in Hz (e.g. 22050 or 44100).
    pub sample_rate: u32,
    /// Source bit resolution (e.g. 16 for standard UE2 USound).
    pub bits_per_sample: u16,
    /// Total duration in seconds.
    pub duration_seconds: f32,
    /// Total number of audio frames (samples per channel).
    pub frame_count: usize,
    /// Interleaved audio samples normalized to [-1.0, 1.0].
    /// For Mono: [M0, M1, M2, ...]
    /// For Stereo: [L0, R0, L1, R1, ...]
    pub samples: Vec<f32>,
}

impl WavSound {
    /// Parses raw RIFF WAVE bytes extracted from a `.uax` package or disk file.
    pub fn parse(bytes: &[u8]) -> Result<Self, WavError> {
        if bytes.len() < 12 {
            return Err(WavError::DataTooShort);
        }

        if &bytes[0..4] != b"RIFF" {
            return Err(WavError::InvalidRiffHeader);
        }

        if &bytes[8..12] != b"WAVE" {
            return Err(WavError::InvalidWaveFormat);
        }

        let mut offset = 12;
        let mut fmt_found = false;
        let mut format_tag: u16 = 0;
        let mut channels: u16 = 0;
        let mut sample_rate: u32 = 0;
        let mut bits_per_sample: u16 = 0;
        let mut data_slice: Option<&[u8]> = None;

        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;

            let data_start = offset + 8;
            let data_end = (data_start + chunk_size).min(bytes.len());

            if chunk_id == b"fmt " {
                if chunk_size < 16 || data_start + 16 > bytes.len() {
                    return Err(WavError::CorruptedChunk);
                }

                format_tag = u16::from_le_bytes([bytes[data_start], bytes[data_start + 1]]);
                channels = u16::from_le_bytes([bytes[data_start + 2], bytes[data_start + 3]]);
                sample_rate = u32::from_le_bytes([
                    bytes[data_start + 4],
                    bytes[data_start + 5],
                    bytes[data_start + 6],
                    bytes[data_start + 7],
                ]);
                bits_per_sample =
                    u16::from_le_bytes([bytes[data_start + 14], bytes[data_start + 15]]);

                // Handle WAVE_FORMAT_EXTENSIBLE (0xFFFE)
                if format_tag == 0xFFFE && chunk_size >= 40 && data_start + 40 <= bytes.len() {
                    let sub_format =
                        u16::from_le_bytes([bytes[data_start + 24], bytes[data_start + 25]]);
                    format_tag = sub_format;
                }

                if format_tag != 1 && format_tag != 3 {
                    return Err(WavError::UnsupportedFormat(format_tag));
                }

                if channels == 0 || channels > 8 {
                    return Err(WavError::UnsupportedChannelCount(channels));
                }

                if bits_per_sample != 8
                    && bits_per_sample != 16
                    && bits_per_sample != 24
                    && bits_per_sample != 32
                {
                    return Err(WavError::UnsupportedBitDepth(bits_per_sample));
                }

                fmt_found = true;
            } else if chunk_id == b"data" {
                data_slice = Some(&bytes[data_start..data_end]);
            }

            // RIFF chunks are padded to 2-byte word boundaries
            let padded_size = (chunk_size + 1) & !1;
            offset = data_start + padded_size;
        }

        if !fmt_found {
            return Err(WavError::MissingFmtChunk);
        }

        let raw_data = data_slice.ok_or(WavError::MissingDataChunk)?;

        // Decode raw bytes to normalized f32 samples
        let samples = decode_pcm_samples(raw_data, format_tag, bits_per_sample)?;
        let frame_count = samples.len() / (channels as usize);
        let duration_seconds = if sample_rate > 0 {
            frame_count as f32 / sample_rate as f32
        } else {
            0.0
        };

        Ok(Self {
            channels,
            sample_rate,
            bits_per_sample,
            duration_seconds,
            frame_count,
            samples,
        })
    }

    /// Constructs a `WavSound` directly from normalized `f32` sample data.
    pub fn from_samples(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        assert!(channels > 0, "channels must be greater than 0");
        let frame_count = samples.len() / (channels as usize);
        let duration_seconds = if sample_rate > 0 {
            frame_count as f32 / sample_rate as f32
        } else {
            0.0
        };

        Self {
            channels,
            sample_rate,
            bits_per_sample: 16,
            duration_seconds,
            frame_count,
            samples,
        }
    }

    /// Encodes this sound as a standard 16-bit PCM RIFF WAVE byte buffer.
    pub fn to_wav_bytes(&self) -> Vec<u8> {
        let block_align = self.channels * 2;
        let byte_rate = self.sample_rate * (block_align as u32);
        let data_size = (self.samples.len() * 2) as u32;
        let file_size = 36 + data_size;

        let mut out = Vec::with_capacity((44 + data_size) as usize);

        // RIFF header
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&file_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");

        // fmt chunk
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // subchunk size = 16 for PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM = 1
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample = 16

        // data chunk
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_size.to_le_bytes());

        for &sample in &self.samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let s16 = (clamped * 32767.0).round() as i16;
            out.extend_from_slice(&s16.to_le_bytes());
        }

        out
    }

    /// Samples the sound at an integer frame index, returning stereo `[left, right]`.
    /// For mono audio, the single channel is mapped equally to both left and right.
    #[inline]
    pub fn get_frame(&self, frame_index: usize) -> Option<[f32; 2]> {
        if frame_index >= self.frame_count {
            return None;
        }

        if self.channels == 1 {
            let s = self.samples[frame_index];
            Some([s, s])
        } else if self.channels == 2 {
            let base = frame_index * 2;
            Some([self.samples[base], self.samples[base + 1]])
        } else {
            // Downmix first two channels or center
            let base = frame_index * (self.channels as usize);
            Some([self.samples[base], self.samples[base + 1]])
        }
    }

    /// Resamples the sound at a fractional frame position using linear interpolation.
    /// Returns stereo `[left, right]`.
    pub fn sample_at_fractional(&self, frame_pos: f64) -> [f32; 2] {
        if self.frame_count == 0 {
            return [0.0, 0.0];
        }

        if frame_pos < 0.0 {
            return self.get_frame(0).unwrap_or([0.0, 0.0]);
        }

        let idx0 = frame_pos.floor() as usize;
        let fract = (frame_pos - frame_pos.floor()) as f32;

        if idx0 >= self.frame_count {
            return [0.0, 0.0];
        }

        let s0 = self.get_frame(idx0).unwrap_or([0.0, 0.0]);
        if idx0 + 1 >= self.frame_count || fract <= 1e-6 {
            return s0;
        }

        let s1 = self.get_frame(idx0 + 1).unwrap_or(s0);
        [
            s0[0] + fract * (s1[0] - s0[0]),
            s0[1] + fract * (s1[1] - s0[1]),
        ]
    }
}

/// Helper function to convert raw PCM byte buffers into normalized `f32` vectors.
fn decode_pcm_samples(
    data: &[u8],
    format_tag: u16,
    bits_per_sample: u16,
) -> Result<Vec<f32>, WavError> {
    if format_tag == 3 {
        // IEEE 32-bit Float
        if bits_per_sample != 32 {
            return Err(WavError::UnsupportedBitDepth(bits_per_sample));
        }
        let count = data.len() / 4;
        let mut samples = Vec::with_capacity(count);
        for chunk in data.as_chunks::<4>().0 {
            let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            samples.push(val.clamp(-1.0, 1.0));
        }
        return Ok(samples);
    }

    // PCM Integer format (format_tag == 1)
    match bits_per_sample {
        8 => {
            // 8-bit unsigned PCM: 0..255, center is 128
            let mut samples = Vec::with_capacity(data.len());
            for &b in data {
                let norm = (b as f32 - 128.0) / 128.0;
                samples.push(norm.clamp(-1.0, 1.0));
            }
            Ok(samples)
        }
        16 => {
            // 16-bit signed PCM (Little-Endian): -32768..32767
            let count = data.len() / 2;
            let mut samples = Vec::with_capacity(count);
            for chunk in data.as_chunks::<2>().0 {
                let val = i16::from_le_bytes([chunk[0], chunk[1]]);
                let norm = val as f32 / 32768.0;
                samples.push(norm.clamp(-1.0, 1.0));
            }
            Ok(samples)
        }
        24 => {
            // 24-bit signed PCM (Little-Endian): -8388608..8388607
            let count = data.len() / 3;
            let mut samples = Vec::with_capacity(count);
            for chunk in data.as_chunks::<3>().0 {
                let sign_extend = if chunk[2] & 0x80 != 0 { 0xFF } else { 0x00 };
                let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], sign_extend]);
                let norm = val as f32 / 8388608.0;
                samples.push(norm.clamp(-1.0, 1.0));
            }
            Ok(samples)
        }
        32 => {
            // 32-bit signed PCM (Little-Endian)
            let count = data.len() / 4;
            let mut samples = Vec::with_capacity(count);
            for chunk in data.as_chunks::<4>().0 {
                let val = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let norm = val as f32 / 2147483648.0;
                samples.push(norm.clamp(-1.0, 1.0));
            }
            Ok(samples)
        }
        _ => Err(WavError::UnsupportedBitDepth(bits_per_sample)),
    }
}
