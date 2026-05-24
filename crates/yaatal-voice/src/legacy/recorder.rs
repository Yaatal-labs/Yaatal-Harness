//! Voice recorder using cpal (legacy path — preserved for emergency revert).
//!
//! [UNVERIFIED] cpal AAudio backend on Android inside Dioxus.
//! This is the Day 1 kill gate — test before building on top of it.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum RecorderError {
    NoDevice,
    BuildStream(String),
    Encoding(String),
}

impl std::fmt::Display for RecorderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecorderError::NoDevice => write!(f, "No audio input device available"),
            RecorderError::BuildStream(e) => write!(f, "Failed to build audio stream: {}", e),
            RecorderError::Encoding(e) => write!(f, "Audio encoding error: {}", e),
        }
    }
}

pub struct VoiceRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl VoiceRecorder {
    pub fn new() -> Result<Self, RecorderError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(RecorderError::NoDevice)?;
        let config = device.default_input_config()
            .map_err(|e| RecorderError::BuildStream(e.to_string()))?;
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
        })
    }

    pub fn start(&self) -> Result<cpal::Stream, RecorderError> {
        // TODO: store device reference to avoid config mismatch
        // The device and config should be reused from `new()` to ensure consistency,
        // but cpal::Device is not Clone/Send. Consider storing device info or refactoring.
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(RecorderError::NoDevice)?;
        let config = device.default_input_config()
            .map_err(|e| RecorderError::BuildStream(e.to_string()))?;
        let samples = self.samples.clone();
        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buffer) = samples.lock() {
                    buffer.extend_from_slice(data);
                }
            },
            |err| tracing::error!("Audio stream error: {}", err),
            None,
        ).map_err(|e| RecorderError::BuildStream(e.to_string()))?;
        stream.play().map_err(|e| RecorderError::BuildStream(e.to_string()))?;
        Ok(stream)
    }

    pub fn stop_and_encode_wav(&self) -> Result<Vec<u8>, RecorderError> {
        let samples = self
            .samples
            .lock()
            .map_err(|e| RecorderError::Encoding(e.to_string()))?
            .clone();
        if samples.is_empty() {
            return Err(RecorderError::Encoding("No audio data recorded".into()));
        }
        let spec = hound::WavSpec {
            channels: self.channels, sample_rate: self.sample_rate,
            bits_per_sample: 32, sample_format: hound::SampleFormat::Float,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| RecorderError::Encoding(e.to_string()))?;
            for sample in &samples {
                writer.write_sample(*sample)
                    .map_err(|e| RecorderError::Encoding(e.to_string()))?;
            }
            writer.finalize().map_err(|e| RecorderError::Encoding(e.to_string()))?;
        }
        Ok(cursor.into_inner())
    }

    pub fn clear(&self) {
        if let Ok(mut s) = self.samples.lock() {
            s.clear();
        }
    }
}
