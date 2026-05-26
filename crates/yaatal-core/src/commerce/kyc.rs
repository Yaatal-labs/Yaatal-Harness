//! KYC profile types — mirrors the `bobo_kyc` table.
//!
//! This table is PII and will be annotated `Sensitivity::Sovereign` in Lane 6.
//! `document_hash` stores a SHA-256 digest; raw documents are never persisted.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// KYC verification status for a participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KycStatus {
    Unverified,
    Pending,
    Verified,
    Rejected,
}

impl KycStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            KycStatus::Unverified => "unverified",
            KycStatus::Pending => "pending",
            KycStatus::Verified => "verified",
            KycStatus::Rejected => "rejected",
        }
    }
}

/// A KYC profile record mirroring the `bobo_kyc` table.
///
/// Construction helpers create values ready for persistence; the `pid` is
/// caller-supplied (typically the user's account UUID).
#[derive(Debug, Clone)]
pub struct KycProfile {
    /// Primary key — participant identifier.
    pub pid: Uuid,
    pub status: KycStatus,
    /// KYC provider name: "smile_id" | "manual" | "none".
    pub provider: String,
    /// Provider-side reference from Smile ID, if applicable.
    pub smile_id_ref: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    /// SHA-256 digest of the submitted identity document (BYTEA in DB).
    /// Raw documents are never stored here.
    pub document_hash: Option<Vec<u8>>,
    /// ISO-3166-1 alpha-2 country code of the issuing jurisdiction.
    pub jurisdiction: String,
    pub created_at: Option<DateTime<Utc>>,
}

impl KycProfile {
    /// Create a new unverified profile ready for insertion.
    pub fn new_unverified(
        pid: Uuid,
        provider: impl Into<String>,
        jurisdiction: impl Into<String>,
    ) -> Self {
        Self {
            pid,
            status: KycStatus::Unverified,
            provider: provider.into(),
            smile_id_ref: None,
            verified_at: None,
            document_hash: None,
            jurisdiction: jurisdiction.into(),
            created_at: None,
        }
    }
}
