//! build.rs for yaatal-voice
//!
//! When the `speech-core-sys` feature is enabled (which is implied by the
//! `models` feature):
//!
//! 1. Runs cmake to build `libspeech_core.a` from `third_party/speech-core`.
//! 2. Optionally builds `libspeech_core_models.a` when `models` feature is set.
//! 3. Runs bindgen against `wrapper.h` to generate `OUT_DIR/speech_core_sys.rs`.
//!
//! When `speech-core-sys` is NOT set (e.g., `cargo check --no-default-features`)
//! the cmake and bindgen steps are skipped entirely, so the crate compiles
//! without any system C++ toolchain or ALSA/ONNX Runtime installed.
//!
//! DOCS_RS guard: docs.rs cross-compiles without native deps — skip all native
//! build steps on that environment too.
//!
//! Lint posture: this is a build script. `expect()` panics on toolchain /
//! cmake / bindgen failures are the correct way to abort a Cargo build; the
//! workspace `expect_used` lint is muted file-wide.

#![allow(clippy::expect_used)]

fn main() {
    // Re-run this script if the upstream C header or CMakeLists changes.
    println!("cargo:rerun-if-changed=wrapper.h");
    println!(
        "cargo:rerun-if-changed=../../third_party/speech-core/include/speech_core/speech_core_c.h"
    );
    println!("cargo:rerun-if-changed=../../third_party/speech-core/CMakeLists.txt");

    // Early-exit on docs.rs (no native toolchain available).
    if std::env::var("DOCS_RS").is_ok() {
        return;
    }

    // Early-exit when the speech-core-sys feature is absent.
    // Cargo sets CARGO_FEATURE_<FEATURE_UPPER_SNAKE> for each enabled feature.
    if std::env::var("CARGO_FEATURE_SPEECH_CORE_SYS").is_err() {
        return;
    }

    build_speech_core();
    generate_bindings();
}

fn build_speech_core() {
    // Locate the submodule root relative to the manifest directory.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let submodule_path = std::path::Path::new(&manifest_dir).join("../../third_party/speech-core");

    let with_onnx = std::env::var("CARGO_FEATURE_MODELS").is_ok();

    let mut cfg = cmake::Config::new(&submodule_path);
    cfg.define("SPEECH_CORE_BUILD_TESTS", "OFF")
        .define("SPEECH_CORE_BUILD_EXAMPLES", "OFF")
        .define(
            "SPEECH_CORE_WITH_ONNX",
            if with_onnx { "ON" } else { "OFF" },
        );

    // Forward ORT_DIR if the caller has it in the environment.
    if let Ok(ort_dir) = std::env::var("ORT_DIR") {
        cfg.define("ORT_DIR", &ort_dir);
    }

    let dst = cfg.build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=speech_core");

    // C++ standard library linkage.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }

    if with_onnx {
        println!("cargo:rustc-link-lib=static=speech_core_models");
        // onnxruntime is a shared library — link dynamically.
        println!("cargo:rustc-link-lib=dylib=onnxruntime");
    }
}

fn generate_bindings() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    let header = std::path::Path::new(&manifest_dir).join("wrapper.h");
    let out_file = std::path::Path::new(&out_dir).join("speech_core_sys.rs");

    let bindings = bindgen::Builder::default()
        .header(header.to_str().expect("non-UTF-8 header path"))
        // Pull in the include path so bindgen resolves the upstream header.
        .clang_arg(format!(
            "-I{}",
            std::path::Path::new(&manifest_dir)
                .join("../../third_party/speech-core/include")
                .display()
        ))
        // Only generate bindings for symbols declared in the C ABI header.
        .allowlist_file(".*speech_core_c\\.h")
        // Derive common traits on generated types.
        .derive_debug(true)
        .derive_default(true)
        .derive_copy(true)
        // Keep enum repr as Rust enum (named variants, not raw integers).
        .rustified_enum("sc_.*_t")
        // Don't generate bindings for system headers included transitively.
        .blocklist_file(".*/stddef\\.h")
        .blocklist_file(".*/stdint\\.h")
        .blocklist_file(".*/stdbool\\.h")
        .generate()
        .expect("bindgen failed to generate speech_core_sys bindings");

    bindings
        .write_to_file(&out_file)
        .expect("failed to write speech_core_sys.rs");
}
