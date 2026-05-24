use hound::{WavSpec, WavWriter};
use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("Hound encoding error: {0}")]
    HoundError(#[from] hound::Error),
}

/// Clamps an f32 sample to [-1.0, 1.0] and converts to i16 for 16-bit PCM.
/// This prevents overflow when samples exceed the normalized range.
#[inline]
fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

/// Encodes raw PCM f32 audio into a 16-bit PCM WAV formatted byte vector.
///
/// Uses 16-bit PCM encoding (not 32-bit float) for compatibility with
/// Whisper and other cloud transcription APIs that expect standard WAV.
/// The f32→i16 conversion includes clamping to prevent overflow.
pub fn encode_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<u8>, CompressError> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut cursor, spec)?;
        for &sample in samples {
            writer.write_sample(f32_to_i16(sample))?;
        }
        writer.finalize()?;
    }

    Ok(cursor.into_inner())
}

/// Placeholder for true bandwidth-crushing Opus encoding.
/// This avoids C++ bindings (`libopus`) during the early build phases,
/// but will be fully implemented when YOKK PWA integration requires it.
pub fn encode_opus(_samples: &[f32]) -> Result<Vec<u8>, CompressError> {
    unimplemented!("Opus encoding requires C-bindings and is deferred to E8/YOKK integration.")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid_16bit_pcm() {
        let samples = vec![0.0f32; 160]; // 10ms at 16kHz
        let wav = encode_wav(&samples, 16000, 1).unwrap();
        // RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        // WAV format
        assert_eq!(&wav[8..12], b"WAVE");
        // Verify it's a valid WAV by reading it back
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.channels, 1);
    }

    #[test]
    fn f32_clamping_prevents_overflow() {
        // Values beyond [-1.0, 1.0] should be clamped
        assert_eq!(f32_to_i16(1.5), i16::MAX);
        assert_eq!(f32_to_i16(-1.5), -i16::MAX);
        assert_eq!(f32_to_i16(0.0), 0);
        // Normal range
        let mid = f32_to_i16(0.5);
        assert!(mid > 0 && mid < i16::MAX);
    }

    #[test]
    fn encode_wav_empty_samples() {
        let wav = encode_wav(&[], 16000, 1).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(&wav)).unwrap();
        assert_eq!(reader.len(), 0);
    }
}
