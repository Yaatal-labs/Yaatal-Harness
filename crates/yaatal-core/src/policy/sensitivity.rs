//! Sensitivity classification for storage residency enforcement (§8.2).
//!
//! # Residency rules
//! - `Sovereign` — stays in Diamniadio Postgres only. Never mirrors to edge.
//! - `Operational` — stored in Postgres; may be cached locally but not mirrored to R2.
//! - `Public` — may be cached on Cloudflare R2 PoPs.
//!
//! The `Tagged<T, S>` newtype couples a value with its sensitivity level at the
//! type level so that compile-time dispatch prevents accidental writes of
//! sovereign data to non-sovereign storage (Lane 6 storage dispatcher).

use std::fmt;
use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Sealed trait plumbing
// ---------------------------------------------------------------------------

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Sovereign {}
    impl Sealed for super::Operational {}
    impl Sealed for super::Public {}
}

// ---------------------------------------------------------------------------
// `Sensitivity` — the runtime enum
// ---------------------------------------------------------------------------

/// Runtime sensitivity level.  Also the value stored inside `Tagged` and
/// returned by `SensitivityTag::LEVEL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Sensitivity {
    /// On-device or Diamniadio Postgres only.  Never leaves the sovereign zone.
    Sovereign,
    /// Postgres-resident.  May be used for operational analytics but not
    /// mirrored to any edge store.
    Operational,
    /// Safe to cache on Cloudflare R2 PoPs or any CDN edge node.
    Public,
}

// ---------------------------------------------------------------------------
// `SensitivityTag` — the compile-time marker trait
// ---------------------------------------------------------------------------

/// Sealed marker trait that associates a compile-time ZST with a runtime
/// [`Sensitivity`] level.
///
/// Implementations are provided for [`Sovereign`], [`Operational`], and
/// [`Public`].  External crates cannot implement this trait (sealed).
pub trait SensitivityTag: sealed::Sealed + 'static {
    /// The runtime level corresponding to this compile-time tag.
    const LEVEL: Sensitivity;
}

// ---------------------------------------------------------------------------
// Zero-sized tag types
// ---------------------------------------------------------------------------

/// Compile-time tag for sovereign data.
#[derive(Debug, Clone, Copy)]
pub struct Sovereign;

/// Compile-time tag for operational data.
#[derive(Debug, Clone, Copy)]
pub struct Operational;

/// Compile-time tag for public data.
#[derive(Debug, Clone, Copy)]
pub struct Public;

impl SensitivityTag for Sovereign {
    const LEVEL: Sensitivity = Sensitivity::Sovereign;
}

impl SensitivityTag for Operational {
    const LEVEL: Sensitivity = Sensitivity::Operational;
}

impl SensitivityTag for Public {
    const LEVEL: Sensitivity = Sensitivity::Public;
}

// ---------------------------------------------------------------------------
// `Tagged<T, S>` — the marker-type newtype
// ---------------------------------------------------------------------------

/// A value of type `T` labelled with sensitivity tag `S` at compile time.
///
/// # Privacy
/// The inner value is intentionally private.  The only way to obtain it is via
/// [`Tagged::into_inner`] (consuming) or [`Tagged::as_ref`] (borrowing).
///
/// # Debug
/// The [`fmt::Debug`] implementation deliberately prints the sensitivity level
/// and a redacted placeholder instead of the inner value, so that `T: Debug`
/// is not required and sensitive data is never accidentally written to logs.
///
/// # Send + Sync
/// `Tagged<T, S>` is `Send + Sync` whenever `T` is `Send + Sync` because
/// `PhantomData<fn() -> S>` is always `Send + Sync`.
pub struct Tagged<T, S: SensitivityTag> {
    value: T,
    _tag: PhantomData<fn() -> S>,
}

impl<T, S: SensitivityTag> Tagged<T, S> {
    /// Wrap `value` with the given sensitivity tag.
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            value,
            _tag: PhantomData,
        }
    }

    /// Return the compile-time sensitivity level for this tagged value.
    #[inline]
    pub fn level(&self) -> Sensitivity {
        S::LEVEL
    }

    /// Consume the wrapper and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }

}

// Manual Clone — only when T: Clone; does NOT require S: Clone (ZSTs are Copy
// but we avoid adding spurious bounds).
impl<T: Clone, S: SensitivityTag> Clone for Tagged<T, S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            _tag: PhantomData,
        }
    }
}

// Implement the standard `AsRef<T>` trait so that callers can borrow the inner
// value via the conventional `as_ref()` call without triggering the
// `clippy::should_implement_trait` warning.
impl<T, S: SensitivityTag> AsRef<T> for Tagged<T, S> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.value
    }
}

// Custom Debug — never prints T, only the sensitivity level.
impl<T, S: SensitivityTag> fmt::Debug for Tagged<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tagged")
            .field("level", &S::LEVEL)
            .field("value", &"<redacted>")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    // -- Serde round-trip on `Sensitivity` -----------------------------------

    #[test]
    fn sensitivity_serde_roundtrip() {
        for variant in [Sensitivity::Sovereign, Sensitivity::Operational, Sensitivity::Public] {
            let json = serde_json::to_string(&variant).expect("serialize");
            let back: Sensitivity = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }

    // -- `Tagged::level()` returns the correct `Sensitivity` for each tag ---

    #[test]
    fn tagged_level_sovereign() {
        let t = Tagged::<String, Sovereign>::new("secret".into());
        assert_eq!(t.level(), Sensitivity::Sovereign);
    }

    #[test]
    fn tagged_level_operational() {
        let t = Tagged::<String, Operational>::new("ops".into());
        assert_eq!(t.level(), Sensitivity::Operational);
    }

    #[test]
    fn tagged_level_public() {
        let t = Tagged::<String, Public>::new("hello".into());
        assert_eq!(t.level(), Sensitivity::Public);
    }

    // -- Compile-time Send + Sync check -------------------------------------

    #[test]
    fn tagged_is_send_sync() {
        fn require_send_sync<T: Send + Sync>() {}
        require_send_sync::<Tagged<String, Sovereign>>();
    }

    // -- Debug impl does NOT leak the inner value ---------------------------

    #[test]
    fn debug_does_not_leak_inner_value() {
        let secret = "super-secret-password";
        let t = Tagged::<&str, Sovereign>::new(secret);
        let debug_output = format!("{t:?}");
        assert!(
            !debug_output.contains(secret),
            "Debug output leaked the inner value: {debug_output}"
        );
        assert!(
            debug_output.contains("<redacted>"),
            "Debug output should contain '<redacted>': {debug_output}"
        );
    }
}
