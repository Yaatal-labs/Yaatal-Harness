use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::pipeline::traits::{ErrorKind, FeedError};

/// Opaque, base64-encoded pagination cursor for resumable feed queries.
/// Serialized as JSON → base64 (URL-safe, no padding).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedCursor {
    pub request_id: String,
    pub last_candidate_id: Option<String>,
    pub offset: usize,
}

impl FeedCursor {
    /// Encodes this cursor as a URL-safe base64 string.
    pub fn encode(&self) -> Result<String, FeedError> {
        let payload = serde_json::to_vec(self).map_err(|error| {
            FeedError::with_kind(
                ErrorKind::Parse,
                "Cursor",
                "FeedCursor",
                format!("failed to serialize cursor: {error}"),
            )
        })?;

        Ok(URL_SAFE_NO_PAD.encode(payload))
    }

    /// Decodes a cursor from a URL-safe base64 string.
    pub fn decode(encoded: &str) -> Result<Self, FeedError> {
        let payload = URL_SAFE_NO_PAD.decode(encoded).map_err(|error| {
            FeedError::with_kind(
                ErrorKind::Parse,
                "Cursor",
                "FeedCursor",
                format!("failed to decode cursor: {error}"),
            )
        })?;

        serde_json::from_slice(&payload).map_err(|error| {
            FeedError::with_kind(
                ErrorKind::Parse,
                "Cursor",
                "FeedCursor",
                format!("failed to parse cursor payload: {error}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::FeedCursor;

    #[test]
    fn round_trip() {
        let cursor = FeedCursor {
            request_id: "req-123".into(),
            last_candidate_id: Some("candidate-9".into()),
            offset: 24,
        };

        let encoded = cursor.encode();
        assert!(encoded.is_ok(), "cursor should encode");

        let decoded = FeedCursor::decode(&encoded.unwrap_or_default());
        assert!(decoded.is_ok(), "cursor should decode");
        assert_eq!(decoded.unwrap_or_default(), cursor);
    }
}
