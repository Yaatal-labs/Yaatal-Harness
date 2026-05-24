use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors that can occur during audio capture.
#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("No input device available")]
    NoDevice,
    #[error("Failed to get supported config: {0}")]
    SupportedStreamConfigError(#[from] cpal::SupportedStreamConfigError),
    #[error("Failed to build stream: {0}")]
    BuildStreamError(#[from] cpal::BuildStreamError),
    #[error("Failed to play stream: {0}")]
    PlayStreamError(#[from] cpal::PlayStreamError),
    #[error("Failed to pause stream: {0}")]
    PauseStreamError(#[from] cpal::PauseStreamError),
    #[error("Stream is already recording")]
    AlreadyRecording,
    #[error("Stream is not recording")]
    NotRecording,
    #[error("Internal lock poisoned")]
    LockPoisoned,
}

/// A service to capture audio from the microphone using standard `cpal`.
///
/// Captures raw f32 PCM samples from the default input device.
/// Use [`compress::encode_wav`] to convert the output to 16-bit PCM WAV
/// for Whisper API compatibility.
pub struct VoiceRecorder {
    #[allow(dead_code)]
    host: cpal::Host,
    device: cpal::Device,
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
    channels: u16,
}

impl VoiceRecorder {
    /// Creates a new `VoiceRecorder` targeting the default system input device.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;
        let default_config = device.default_input_config()?;
        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();

        Ok(Self {
            host,
            device,
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate,
            channels,
        })
    }

    /// Returns `true` if currently recording.
    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }

    /// Returns the number of samples captured so far.
    pub fn sample_count(&self) -> Result<usize, CaptureError> {
        let buf = self.buffer.lock().map_err(|_| CaptureError::LockPoisoned)?;
        Ok(buf.len())
    }

    /// Returns the sample rate (Hz) of the input device.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of audio channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Clears the internal sample buffer without stopping recording.
    pub fn clear(&mut self) -> Result<(), CaptureError> {
        let mut buf = self.buffer.lock().map_err(|_| CaptureError::LockPoisoned)?;
        buf.clear();
        Ok(())
    }

    /// Starts recording audio into the internal buffer.
    pub fn start(&mut self) -> Result<(), CaptureError> {
        if self.stream.is_some() {
            return Err(CaptureError::AlreadyRecording);
        }

        let config = self.device.default_input_config()?;
        let sample_format = config.sample_format();

        // Warn if device config differs from what we stored at construction time
        if config.sample_rate().0 != self.sample_rate || config.channels() != self.channels {
            tracing::warn!(
                "Device config mismatch: expected {}Hz/{}ch, got {}Hz/{}ch",
                self.sample_rate,
                self.channels,
                config.sample_rate().0,
                config.channels()
            );
        }

        let config: cpal::StreamConfig = config.into();
        let buffer_clone = Arc::clone(&self.buffer);

        let err_fn = move |err| {
            tracing::error!("an error occurred on stream: {}", err);
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => self.device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer_clone.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )?,
            _ => {
                return Err(CaptureError::BuildStreamError(
                    cpal::BuildStreamError::FormatNotSupported,
                ))
            }
        };

        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }

    /// Stops recording and returns the raw f32 PCM buffer.
    pub fn stop(&mut self) -> Result<Vec<f32>, CaptureError> {
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
            let mut buf = self.buffer.lock().map_err(|_| CaptureError::LockPoisoned)?;
            let data = std::mem::take(&mut *buf);
            Ok(data)
        } else {
            Err(CaptureError::NotRecording)
        }
    }
}
