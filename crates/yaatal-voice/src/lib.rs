//! `yaatal-voice` — voice pipeline facade for the Yaatal Engine.
//!
//! This crate provides a safe Rust wrapper over the `soniqo/speech-core` C++
//! pipeline via the C ABI (`speech_core_c.h`).
//!
//! # Features
//!
//! | Feature | Default | Purpose |
//! |---|---|---|
//! | `speech-core-sys` | off | Enable cmake build + bindgen bindings for the real C++ pipeline |
//! | `models` | off | Also build `libspeech_core_models.a` + link system onnxruntime |
//! | `device-io` | off | Pull in `cpal` for audio-device capture (Linux/macOS dev) |
//! | `legacy-cpal-whisper` | off | Restore the old cpal + HF-Whisper path for emergency revert |
//!
//! For server builds (raw WebSocket PCM input) use `--no-default-features`.
//! For local device capture enable `device-io`.
//! For the full ONNX-backed model suite enable `models`.

// ---------------------------------------------------------------------------
// Raw FFI bindings (real or mock depending on `speech-core-sys` feature)
// ---------------------------------------------------------------------------
pub mod sys;

// ---------------------------------------------------------------------------
// Safe Rust facade over the C++ pipeline
// ---------------------------------------------------------------------------
pub mod session;

// Re-export the most-used types at crate root for ergonomics.
pub use session::{BuildError, Event, Session, SessionBuilder};

// ---------------------------------------------------------------------------
// Legacy cpal + HF-Whisper backend (preserved for emergency revert)
//
// Gate: `legacy-cpal-whisper` feature (off by default)
// ---------------------------------------------------------------------------
#[cfg(feature = "legacy-cpal-whisper")]
pub mod legacy;
