//! Network condition detection and gating for the AI cascade router.
//!
//! Tier 1 (on-device) is always available. Tiers 2-5 require 3G or better.
//! The [`NetworkGate`] trait is injectable so tests can mock connectivity.

use std::fmt;

/// Observed network condition, ordered from worst to best.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetworkCondition {
    /// No connectivity at all.
    Offline = 0,
    /// GPRS / EDGE — high latency, very low throughput.
    TwoG = 1,
    /// HSPA / UMTS — usable for small API payloads.
    ThreeG = 2,
    /// LTE / Wi-Fi — full capability.
    FourGPlus = 3,
}

impl fmt::Display for NetworkCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "Offline"),
            Self::TwoG => write!(f, "2G"),
            Self::ThreeG => write!(f, "3G"),
            Self::FourGPlus => write!(f, "4G+"),
        }
    }
}

/// Trait for probing current network quality.
///
/// Implementations may check OS APIs, ping endpoints, or return a
/// hard-coded value (useful for tests and early development).
pub trait NetworkGate: Send + Sync {
    /// Return the current network condition.
    fn condition(&self) -> NetworkCondition;
}

/// Default gate that always reports full connectivity.
///
/// Production-grade detection will land in E7/E8 when the mobile
/// runtime (Dioxus) provides real connectivity callbacks.
#[derive(Debug, Clone, Copy)]
pub struct DefaultNetworkGate;

impl NetworkGate for DefaultNetworkGate {
    fn condition(&self) -> NetworkCondition {
        NetworkCondition::FourGPlus
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ordering_offline_is_worst() {
        assert!(NetworkCondition::Offline < NetworkCondition::TwoG);
        assert!(NetworkCondition::TwoG < NetworkCondition::ThreeG);
        assert!(NetworkCondition::ThreeG < NetworkCondition::FourGPlus);
    }

    #[test]
    fn display_formats() {
        assert_eq!(format!("{}", NetworkCondition::Offline), "Offline");
        assert_eq!(format!("{}", NetworkCondition::TwoG), "2G");
        assert_eq!(format!("{}", NetworkCondition::ThreeG), "3G");
        assert_eq!(format!("{}", NetworkCondition::FourGPlus), "4G+");
    }

    #[test]
    fn default_gate_returns_four_g_plus() {
        let gate = DefaultNetworkGate;
        assert_eq!(gate.condition(), NetworkCondition::FourGPlus);
    }
}
