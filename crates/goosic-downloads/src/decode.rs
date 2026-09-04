//! Turning a downloaded WebM/Opus file into audio the platform can play.
//!
//! macOS cannot play these files as they are: AVFoundation has no Opus or WebM support, and
//! WKWebView refuses to load them even though `canPlayType` claims otherwise, because WebKit's
//! WebM support is behind a feature flag that is off for embedded web views. Rather than reach
//! for a private toggle, the decode happens here and the shell is handed plain PCM in a WAV
//! container, which every platform can play.

use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, Write};
use std::path::Path;

use matroska_demuxer::{Frame, MatroskaFile, TrackType};
use opus_pure::OpusDecoder;

use crate::DownloadError;

/// Opus always decodes at 48 kHz; the container's sampling frequency describes the source, not
/// the decoder's output.
pub const OUTPUT_SAMPLE_RATE: u32 = 48_000;
/// The largest frame Opus can produce per channel (120 ms at 48 kHz).
const MAX_FRAME_SAMPLES: usize = 5_760;
/// A guard against a malformed or hostile file decoding into unbounded memory: eight hours.
const MAX_OUTPUT_SAMPLES: usize = OUTPUT_SAMPLE_RATE as usize * 3_600 * 8;

/// Decoded audio, ready to be written as WAV.
pub struct DecodedAudio {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples.
    pub samples: Vec<f32>,
}

impl DecodedAudio {
    pub fn duration_seconds(&self) -> f64 {
        if self.channels == 0 || self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / self.channels as f64 / self.sample_rate as f64
    }
}

/// Decodes the first Opus audio track of a WebM/Matroska file.
pub fn decode_opus_webm(path: &Path) -> Result<DecodedAudio, DownloadError> {
    let file = File::open(path).map_err(|error| DownloadError::Unreadable {
        path: path.display().to_string(),
        source: error,
    })?;
    let mut matroska = MatroskaFile::open(BufReader::new(file))
        .map_err(|error| DownloadError::Undecodable(format!("container: {error}")))?;

    let track = matroska
        .tracks()
        .iter()
        .find(|track| track.track_type() == TrackType::Audio && track.codec_id() == "A_OPUS")
        .ok_or_else(|| DownloadError::Undecodable("no Opus audio track".into()))?;
    let track_number = track.track_number().get();
    let channels = track
        .audio()
        .map(|audio| audio.channels().get())
        .unwrap_or(2)
        .clamp(1, 2) as u16;
    // `CodecDelay`/`SeekPreRoll` samples are encoder priming and must be dropped, or every
    // track starts with a short burst of noise.
    let pre_skip = pre_skip_samples(track.codec_private());

    let mut decoder = OpusDecoder::new(OUTPUT_SAMPLE_RATE as i32, channels as usize)
        .map_err(|error| DownloadError::Undecodable(format!("opus decoder: {error}")))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut scratch = vec![0.0f32; MAX_FRAME_SAMPLES * channels as usize];
    let mut frame = Frame::default();
    while matroska
        .next_frame(&mut frame)
        .map_err(|error| DownloadError::Undecodable(format!("frame: {error}")))?
    {
        if frame.track != track_number || frame.data.is_empty() {
            continue;
        }
        let decoded = decoder
            .decode(&frame.data, MAX_FRAME_SAMPLES, &mut scratch)
            .map_err(|error| DownloadError::Undecodable(format!("opus frame: {error}")))?;
        let produced = decoded * channels as usize;
        if samples.len() + produced > MAX_OUTPUT_SAMPLES {
            return Err(DownloadError::Undecodable(
                "file decodes to an implausible length".into(),
            ));
        }
        samples.extend_from_slice(&scratch[..produced]);
    }

    if samples.is_empty() {
        return Err(DownloadError::Undecodable("track produced no audio".into()));
    }

    let skip = (pre_skip as usize)
        .saturating_mul(channels as usize)
        .min(samples.len());
    samples.drain(..skip);

    Ok(DecodedAudio {
        channels,
        sample_rate: OUTPUT_SAMPLE_RATE,
        samples,
    })
}

/// Reads the pre-skip field of an `OpusHead` codec-private block, in 48 kHz samples.
fn pre_skip_samples(codec_private: Option<&[u8]>) -> u16 {
    let header = codec_private.unwrap_or_default();
    if header.len() < 12 || &header[..8] != b"OpusHead" {
        return 0;
    }
    u16::from_le_bytes([header[10], header[11]])
}

/// Writes 16-bit PCM WAV.
///
/// The file is written beside its destination and renamed, so an interrupted decode cannot
/// leave a half-written file that later looks like a valid cache entry.
pub fn write_wav(audio: &DecodedAudio, destination: &Path) -> Result<(), DownloadError> {
    let temporary = destination.with_extension("wav.partial");
    let unwritable = |error: std::io::Error| DownloadError::Unwritable {
        path: destination.display().to_string(),
        source: error,
    };

    {
        let mut writer = BufWriter::new(File::create(&temporary).map_err(unwritable)?);
        let channels = audio.channels;
        let bits = 16u16;
        let block_align = channels * bits / 8;
        let byte_rate = audio.sample_rate * block_align as u32;
        let data_bytes = (audio.samples.len() * 2) as u32;

        writer.write_all(b"RIFF").map_err(unwritable)?;
        writer
            .write_all(&(36 + data_bytes).to_le_bytes())
            .map_err(unwritable)?;
        writer.write_all(b"WAVEfmt ").map_err(unwritable)?;
        writer.write_all(&16u32.to_le_bytes()).map_err(unwritable)?;
        writer.write_all(&1u16.to_le_bytes()).map_err(unwritable)?;
        writer
            .write_all(&channels.to_le_bytes())
            .map_err(unwritable)?;
        writer
            .write_all(&audio.sample_rate.to_le_bytes())
            .map_err(unwritable)?;
        writer
            .write_all(&byte_rate.to_le_bytes())
            .map_err(unwritable)?;
        writer
            .write_all(&block_align.to_le_bytes())
            .map_err(unwritable)?;
        writer.write_all(&bits.to_le_bytes()).map_err(unwritable)?;
        writer.write_all(b"data").map_err(unwritable)?;
        writer
            .write_all(&data_bytes.to_le_bytes())
            .map_err(unwritable)?;

        for sample in &audio.samples {
            // Opus decodes outside [-1, 1] on loud material; clamping here is what keeps that
            // from wrapping around into a click.
            let clamped = sample.clamp(-1.0, 1.0);
            let value = (clamped * i16::MAX as f32).round() as i16;
            writer.write_all(&value.to_le_bytes()).map_err(unwritable)?;
        }
        writer.flush().map_err(unwritable)?;
        writer.get_mut().sync_all().map_err(unwritable)?;
        writer.get_mut().rewind().map_err(unwritable)?;
    }

    std::fs::rename(&temporary, destination).map_err(unwritable)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_skip_is_read_from_an_opus_head_and_ignored_otherwise() {
        let mut header = b"OpusHead".to_vec();
        header.extend_from_slice(&[1, 2]); // version, channel count
        header.extend_from_slice(&312u16.to_le_bytes());
        assert_eq!(pre_skip_samples(Some(&header)), 312);
        assert_eq!(pre_skip_samples(None), 0);
        assert_eq!(pre_skip_samples(Some(b"not a header")), 0);
        assert_eq!(pre_skip_samples(Some(b"OpusHea")), 0);
    }

    #[test]
    fn wav_output_has_a_correct_header_and_sample_count() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("out.wav");
        let audio = DecodedAudio {
            channels: 2,
            sample_rate: 48_000,
            samples: vec![0.0, 0.5, -0.5, 1.0],
        };
        write_wav(&audio, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2, "channels");
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            48_000,
            "sample rate"
        );
        assert_eq!(bytes.len(), 44 + audio.samples.len() * 2);
        assert!(!path.with_extension("wav.partial").exists());
    }

    #[test]
    fn samples_beyond_full_scale_clamp_rather_than_wrap() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loud.wav");
        write_wav(
            &DecodedAudio {
                channels: 1,
                sample_rate: 48_000,
                samples: vec![2.0, -2.0],
            },
            &path,
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(i16::from_le_bytes([bytes[44], bytes[45]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), -i16::MAX);
    }

    #[test]
    fn duration_is_derived_from_the_sample_count() {
        let audio = DecodedAudio {
            channels: 2,
            sample_rate: 48_000,
            samples: vec![0.0; 48_000 * 2],
        };
        assert!((audio.duration_seconds() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_file_that_is_not_a_container_is_reported_rather_than_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-webm.webm");
        std::fs::write(&path, b"this is not a matroska file").unwrap();
        let error = decode_opus_webm(&path).map(|_| ()).unwrap_err();
        assert!(matches!(error, DownloadError::Undecodable(_)));
    }
}
