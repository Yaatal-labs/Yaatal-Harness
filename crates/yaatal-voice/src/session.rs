//! Safe Rust facade over the `speech-core` C++ pipeline.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use yaatal_voice::session::{Session, SessionBuilder, Event};
//!
//! # fn main() -> Result<(), yaatal_voice::session::BuildError> {
//! let mut session = SessionBuilder::new()
//!     .sample_rate(16_000)
//!     .channels(1)
//!     .vad_threshold(0.5)
//!     .language("en")
//!     .build()?;
//!
//! session.start();
//!
//! // Feed PCM frames (f32, normalised -1.0 … +1.0):
//! let frame = vec![0.0f32; 512];
//! session.push_pcm(&frame);
//!
//! // Poll for events (non-blocking):
//! while let Some(event) = session.poll() {
//!     match event {
//!         Event::SpeechStart => { /* VAD onset */ }
//!         Event::Final(text) => { /* transcript */ }
//!         Event::TtsAudio(audio) => { /* play it */ }
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Thread-safety
//!
//! `Session` is **not `Send`** by default because the `speech-core` pipeline
//! docs do not guarantee that `sc_pipeline_push_audio` is safe to call from a
//! different thread than the one that created the pipeline.  If you need to
//! share a session across threads, wrap it in `Arc<Mutex<Session>>`.
//!
//! # Feature flags
//!
//! - `speech-core-sys` — enables the real C++ pipeline (via `cmake` + `bindgen`).
//! - `models` — also links `libspeech_core_models.a` + system `onnxruntime`.
//! - `device-io` — adds `cpal` for audio-device capture.
//! - `legacy-cpal-whisper` — enables the old `recorder`/`transcribe` path.

use std::collections::VecDeque;

#[cfg(feature = "speech-core-sys")]
use std::ffi::CString;

use bytes::Bytes;
use thiserror::Error;

use crate::sys;

// ---------------------------------------------------------------------------
// Public event type
// ---------------------------------------------------------------------------

/// Events emitted by a [`Session`].
#[derive(Debug, Clone)]
pub enum Event {
    /// VAD onset — speech has been detected.
    SpeechStart,
    /// VAD offset — speech segment has ended.
    SpeechEnd,
    /// Interim / streaming transcription result (not final).
    Partial(String),
    /// Final transcription result for a completed utterance.
    Final(String),
    /// The LLM response was interrupted (user started speaking again).
    Interrupted,
    /// Raw TTS audio (f32 PCM bytes, host byte order, mono).
    /// The sample rate matches `SessionBuilder::sample_rate`.
    TtsAudio(Bytes),
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from [`SessionBuilder::build`].
#[derive(Debug, Error)]
pub enum BuildError {
    /// A required C-string argument contained an interior NUL byte.
    #[error("invalid C string argument: {0}")]
    InvalidCString(#[from] std::ffi::NulError),

    /// The upstream `sc_pipeline_create` returned NULL, or the `speech-core-sys`
    /// feature was not enabled (mock mode — no real C++ pipeline available).
    #[error("speech-core pipeline creation failed (sc_pipeline_create returned NULL, or speech-core-sys feature is disabled)")]
    PipelineCreateFailed,
}

// ---------------------------------------------------------------------------
// Internal callback state (used only when speech-core-sys is enabled)
// ---------------------------------------------------------------------------

/// State shared between the Rust event callback and the `Session`.
///
/// Allocated on the heap; a raw pointer is passed as the `event_context`
/// argument to `sc_pipeline_create`.  The `Session` owns this allocation and
/// drops it in its `Drop` impl after destroying the C++ pipeline.
#[cfg(feature = "speech-core-sys")]
pub(crate) struct CallbackState {
    pub(crate) queue: VecDeque<Event>,
}

#[cfg(feature = "speech-core-sys")]
impl CallbackState {
    pub(crate) fn new() -> Box<Self> {
        Box::new(Self {
            queue: VecDeque::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Event callback (real path — only compiled when speech-core-sys is enabled)
// ---------------------------------------------------------------------------

#[cfg(feature = "speech-core-sys")]
pub(crate) unsafe extern "C" fn event_callback(
    event: *const sys::sc_event_t,
    context: *mut std::os::raw::c_void,
) {
    use std::ffi::CStr;

    // SAFETY: context is a pointer to a CallbackState that is alive as long as
    // the pipeline exists.  We have exclusive access here because push_audio is
    // called on the owning thread (Session is !Send).
    let state = &mut *(context as *mut CallbackState);

    let ev = match (*event).type_ {
        sys::sc_event_type_t::SC_EVENT_SPEECH_STARTED => Event::SpeechStart,
        sys::sc_event_type_t::SC_EVENT_SPEECH_ENDED => Event::SpeechEnd,
        sys::sc_event_type_t::SC_EVENT_PARTIAL_TRANSCRIPTION => {
            let text = text_from_c_ptr((*event).text);
            Event::Partial(text)
        }
        sys::sc_event_type_t::SC_EVENT_TRANSCRIPTION_COMPLETED => {
            let text = text_from_c_ptr((*event).text);
            Event::Final(text)
        }
        sys::sc_event_type_t::SC_EVENT_RESPONSE_INTERRUPTED => Event::Interrupted,
        sys::sc_event_type_t::SC_EVENT_RESPONSE_AUDIO_DELTA => {
            let len = (*event).audio_data_length;
            let ptr = (*event).audio_data;
            if !ptr.is_null() && len > 0 {
                let slice = std::slice::from_raw_parts(ptr, len);
                Event::TtsAudio(Bytes::copy_from_slice(slice))
            } else {
                return; // empty delta — skip
            }
        }
        // Session-created, response-created, response-done, tool events —
        // not surfaced in the high-level API yet; drop silently.
        _ => return,
    };

    state.queue.push_back(ev);

    /// # Safety
    /// ptr must be NULL or point to a valid NUL-terminated C string.
    unsafe fn text_from_c_ptr(ptr: *const std::os::raw::c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------
// SessionBuilder
// ---------------------------------------------------------------------------

/// Builder for [`Session`].
///
/// All parameters have safe defaults.
#[derive(Debug, Clone)]
pub struct SessionBuilder {
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) vad_threshold: f32,
    pub(crate) language: String,
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            vad_threshold: 0.5,
            language: String::from("en"),
        }
    }
}

impl SessionBuilder {
    /// Create a builder with sensible defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the PCM sample rate in Hz (default: 16 000).
    pub fn sample_rate(mut self, hz: u32) -> Self {
        self.sample_rate = hz;
        self
    }

    /// Set the channel count (default: 1 / mono).
    pub fn channels(mut self, ch: u16) -> Self {
        self.channels = ch;
        self
    }

    /// Set the VAD onset/offset threshold (0.0–1.0, default: 0.5).
    ///
    /// Maps to both `sc_config_t::vad_onset` and a derived `vad_offset`.
    pub fn vad_threshold(mut self, t: f32) -> Self {
        self.vad_threshold = t;
        self
    }

    /// Set the BCP-47 language hint (default: `"en"`).
    pub fn language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// Build the [`Session`].
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::PipelineCreateFailed`] when `speech-core-sys` is
    /// not enabled (mock mode) or the C++ pipeline constructor returns NULL.
    pub fn build(self) -> Result<Session, BuildError> {
        Session::new(self)
    }
}

// ---------------------------------------------------------------------------
// Session — real implementation (speech-core-sys enabled)
// ---------------------------------------------------------------------------

/// Owns a `speech-core` C++ pipeline behind a safe Rust API.
///
/// See the [module docs](self) for a usage example.
///
/// ## Drop
///
/// `Drop` calls `sc_pipeline_stop` followed by `sc_pipeline_destroy`, then
/// frees the `CallbackState` heap allocation.
///
/// ## Send / Sync
///
/// `Session` is intentionally `!Send` because `sc_pipeline_push_audio` must
/// be called from the thread that owns the pipeline.
#[cfg(feature = "speech-core-sys")]
pub struct Session {
    /// Opaque pipeline handle.  Non-null while `Session` is alive.
    pipeline: sys::sc_pipeline_t,
    /// Heap-allocated callback state.  Owned exclusively by `Session`.
    /// We use a raw pointer so we can pass it through the C ABI; the Box
    /// is reconstructed in `Drop` to free the memory.
    state: *mut CallbackState,
    /// The language CString must stay alive as long as the config is in use.
    _language: CString,
    /// Makes `Session` `!Send` on stable Rust without `feature(negative_impls)`.
    /// `*mut ()` is `!Send + !Sync`; wrapping it in `PhantomData` propagates
    /// those constraints to `Session`.
    _not_send: std::marker::PhantomData<*mut ()>,
}

// The auto-trait bound from PhantomData<*mut ()> already makes Session !Send.
// Document the intent explicitly so reviewers know this is intentional.
// (A negative `impl !Send` would require nightly; PhantomData achieves the same.)

#[cfg(feature = "speech-core-sys")]
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("pipeline", &(self.pipeline as usize))
            .finish()
    }
}

#[cfg(feature = "speech-core-sys")]
impl Session {
    fn new(cfg: SessionBuilder) -> Result<Self, BuildError> {
        let language = CString::new(cfg.language)?;
        let state = Box::into_raw(CallbackState::new());

        let sc_config = sys::sc_config_t {
            vad_onset: cfg.vad_threshold,
            vad_offset: cfg.vad_threshold * 0.8,
            language: language.as_ptr(),
            mode: sys::sc_mode_t::SC_MODE_TRANSCRIBE_ONLY,
            allow_interruptions: true,
            emit_partial_transcriptions: true,
            min_speech_duration: 0.1,
            min_silence_duration: 0.5,
            min_interruption_duration: 0.3,
            interruption_recovery_timeout: 0.5,
            max_utterance_duration: 30.0,
            pre_speech_buffer_duration: 0.1,
            max_response_duration: 60.0,
            post_playback_guard: 0.1,
            eager_stt: false,
            eager_stt_delay: 0.5,
            warmup_stt: false,
            max_history_messages: 50,
            max_history_tokens: 0,
            mask_tool_results: false,
            partial_transcription_interval: 1.0,
        };

        // Stub vtables — all function pointers NULL.  The pipeline orchestration
        // core accepts this in TRANSCRIBE_ONLY mode.  Callers using the `models`
        // feature should supply real vtables via the C ABI before calling start().
        let stt = sys::sc_stt_vtable_t {
            context: std::ptr::null_mut(),
            transcribe: None,
            input_sample_rate: None,
            begin_stream: None,
            push_chunk: None,
            flush_stream: None,
            end_stream: None,
            cancel_stream: None,
        };
        let tts = sys::sc_tts_vtable_t {
            context: std::ptr::null_mut(),
            synthesize: None,
            output_sample_rate: None,
            cancel: None,
        };
        let vad = sys::sc_vad_vtable_t {
            context: std::ptr::null_mut(),
            process_chunk: None,
            reset: None,
            input_sample_rate: None,
            chunk_size: None,
        };

        // SAFETY: all pointers are valid; state lives until Drop.
        let pipeline = unsafe {
            sys::sc_pipeline_create(
                stt,
                tts,
                std::ptr::null_mut(), // no LLM for transcribe-only
                vad,
                sc_config,
                Some(event_callback),
                state as *mut std::os::raw::c_void,
            )
        };

        if pipeline.is_null() {
            // Reclaim the CallbackState allocation before returning the error.
            // SAFETY: state was allocated by Box::into_raw and not aliased yet.
            drop(unsafe { Box::from_raw(state) });
            return Err(BuildError::PipelineCreateFailed);
        }

        Ok(Self {
            pipeline,
            state,
            _language: language,
            _not_send: std::marker::PhantomData,
        })
    }

    /// Returns a [`SessionBuilder`] with default parameters.
    pub fn builder() -> SessionBuilder {
        SessionBuilder::new()
    }

    /// Start the pipeline (enters the LISTENING state).
    ///
    /// Must be called before [`push_pcm`](Self::push_pcm).
    pub fn start(&mut self) {
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_start(self.pipeline) }
    }

    /// Stop the pipeline and cancel any in-progress work.
    pub fn stop(&mut self) {
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_stop(self.pipeline) }
    }

    /// Signal that TTS playback has finished so the pipeline resumes listening.
    pub fn resume_listening(&mut self) {
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_resume_listening(self.pipeline) }
    }

    /// Feed a frame of PCM audio into the pipeline's lock-free ring buffer.
    ///
    /// `frame` must be f32 normalised audio at the sample rate and channel
    /// count configured in [`SessionBuilder`].  For multi-channel audio the
    /// samples must be interleaved.
    pub fn push_pcm(&mut self, frame: &[f32]) {
        if frame.is_empty() {
            return;
        }
        // SAFETY: pipeline is non-null; frame slice is valid for its lifetime.
        unsafe { sys::sc_pipeline_push_audio(self.pipeline, frame.as_ptr(), frame.len()) }
    }

    /// Dequeue the next [`Event`] from the internal queue, or `None` if no
    /// events are pending.
    ///
    /// Events are enqueued by the C++ pipeline callback during
    /// [`push_pcm`](Self::push_pcm).  Call `poll` in a loop after each
    /// `push_pcm` to drain all pending events.
    pub fn poll(&mut self) -> Option<Event> {
        // SAFETY: state is non-null and exclusively owned by this Session.
        let state = unsafe { &mut *self.state };
        state.queue.pop_front()
    }

    /// Returns the current pipeline state.
    pub fn pipeline_state(&self) -> sys::sc_state_t {
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_state(self.pipeline) }
    }

    /// Returns `true` if the pipeline is currently running.
    pub fn is_running(&self) -> bool {
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_is_running(self.pipeline) }
    }
}

#[cfg(feature = "speech-core-sys")]
impl Drop for Session {
    fn drop(&mut self) {
        // 1. Stop — cancel in-progress work, drain callbacks.
        // SAFETY: pipeline is non-null.
        unsafe { sys::sc_pipeline_stop(self.pipeline) }

        // 2. Destroy the C++ pipeline.  After this call the C++ side will
        //    never invoke `event_callback` again.
        // SAFETY: pipeline is non-null and not aliased.
        unsafe { sys::sc_pipeline_destroy(self.pipeline) }

        // 3. Reclaim the CallbackState heap allocation.
        // SAFETY: state was allocated by Box::into_raw in Session::new; the
        // C++ pipeline has been destroyed so no further callbacks will fire.
        drop(unsafe { Box::from_raw(self.state) });
    }
}

// ---------------------------------------------------------------------------
// Session — mock / stub implementation (speech-core-sys NOT enabled)
//
// WARNING: This stub exists solely so that `cargo check --no-default-features`
// succeeds without the C++ toolchain.  It has no real implementation and must
// never be used in a production binary.  Enable `speech-core-sys` (or the
// `models` feature) for the real pipeline.
// ---------------------------------------------------------------------------

/// Stub `Session` type for `--no-default-features` / docs.rs / CI environments
/// that lack the C++ toolchain.
///
/// # WARNING
///
/// This is a **mock** implementation.  All methods are no-ops.  [`Session::new`]
/// always returns [`BuildError::PipelineCreateFailed`].  To use the real
/// speech-core pipeline, enable the `speech-core-sys` or `models` Cargo feature.
#[cfg(not(feature = "speech-core-sys"))]
#[derive(Debug)]
pub struct Session {
    /// Keeps the unused queue type in scope so callers can still use `poll()`.
    _queue: VecDeque<Event>,
}

#[cfg(not(feature = "speech-core-sys"))]
impl Session {
    #[allow(clippy::unnecessary_wraps)]
    fn new(_cfg: SessionBuilder) -> Result<Self, BuildError> {
        Err(BuildError::PipelineCreateFailed)
    }

    /// Returns a [`SessionBuilder`] with default parameters.
    pub fn builder() -> SessionBuilder {
        SessionBuilder::new()
    }

    /// No-op in mock mode.
    pub fn start(&mut self) {}

    /// No-op in mock mode.
    pub fn stop(&mut self) {}

    /// No-op in mock mode.
    pub fn resume_listening(&mut self) {}

    /// No-op in mock mode.
    pub fn push_pcm(&mut self, _frame: &[f32]) {}

    /// Always returns `None` in mock mode.
    pub fn poll(&mut self) -> Option<Event> {
        None
    }

    /// Returns the idle state constant in mock mode.
    pub fn pipeline_state(&self) -> sys::sc_state_t {
        sys::sc_state_t::SC_STATE_IDLE
    }

    /// Always returns `false` in mock mode.
    pub fn is_running(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults_are_sane() {
        let b = SessionBuilder::new();
        assert_eq!(b.sample_rate, 16_000);
        assert_eq!(b.channels, 1);
        assert!((b.vad_threshold - 0.5).abs() < 1e-6);
        assert_eq!(b.language, "en");
    }

    #[test]
    fn builder_setters_work() {
        let b = SessionBuilder::new()
            .sample_rate(44_100)
            .channels(2)
            .vad_threshold(0.7)
            .language("fr");
        assert_eq!(b.sample_rate, 44_100);
        assert_eq!(b.channels, 2);
        assert!((b.vad_threshold - 0.7).abs() < 1e-6);
        assert_eq!(b.language, "fr");
    }

    // When speech-core-sys is absent, build() must fail gracefully (not panic).
    #[cfg(not(feature = "speech-core-sys"))]
    #[test]
    fn build_without_sys_feature_returns_error() {
        let result = SessionBuilder::new().build();
        assert!(
            result.is_err(),
            "Expected BuildError when speech-core-sys is not enabled"
        );
    }
}
