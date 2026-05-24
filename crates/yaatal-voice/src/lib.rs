//! `yaatal-voice` — voice pipeline for the Yaatal Engine.
//!
//! Two complementary surfaces:
//!
//! 1. **speech-core facade** (`session`, `sys`) — safe Rust wrapper over the
//!    `soniqo/speech-core` C++ pipeline via the C ABI (`speech_core_c.h`).
//!    Gated by the `speech-core-sys` feature for the real build; ships mock
//!    bindings otherwise so `cargo check --no-default-features` is green.
//! 2. **WebSocket transport** (`backend`, `server`, `client`, `transcribe`,
//!    `contracts`, `compress`, `mock`) — the production HTTP/WS surface that
//!    the API layer consumes. PCM in, transcripts/events out.
//!
//! # Features
//!
//! | Feature | Default | Purpose |
//! |---|---|---|
//! | `speech-core-sys` | off | Enable cmake build + bindgen bindings for the real C++ pipeline |
//! | `models` | off | Also build `libspeech_core_models.a` + link system onnxruntime |
//! | `edge` | off | Pull in `cpal` for audio-device capture (Linux/macOS dev, kiosks) |
//! | `legacy-cpal-whisper` | off | Restore the old cpal + HF-Whisper path for emergency revert |
//!
//! For server builds (raw WebSocket PCM input) use `--no-default-features`.

// ---------------------------------------------------------------------------
// WebSocket transport + production voice surface
// ---------------------------------------------------------------------------
pub mod backend;
#[cfg(feature = "edge")]
pub mod capture;
pub mod client;
pub mod compress;
pub mod contracts;
pub mod mock;
pub mod server;
pub mod transcribe;

// ---------------------------------------------------------------------------
// speech-core C-ABI facade (real or mock depending on `speech-core-sys` feature)
// ---------------------------------------------------------------------------
pub mod session;
pub mod sys;

pub use session::{BuildError, Event, Session, SessionBuilder};

// ---------------------------------------------------------------------------
// Legacy cpal + HF-Whisper backend (preserved for emergency revert)
// ---------------------------------------------------------------------------
#[cfg(feature = "legacy-cpal-whisper")]
pub mod legacy;
