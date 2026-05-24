//! Legacy cpal + HF-Whisper backend.
//!
//! This module is preserved for emergency revert purposes.  It is gated behind
//! the `legacy-cpal-whisper` Cargo feature which implies `device-io` (cpal)
//! and `hound` (WAV encoding).
//!
//! To use the legacy path, enable the feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! yaatal-voice = { path = "...", features = ["legacy-cpal-whisper"] }
//! ```
//!
//! The legacy feature is **off by default** and is not enabled in server builds.

pub mod recorder;
pub mod transcribe;
