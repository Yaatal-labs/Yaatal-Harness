//! Sensitive content detection
//! Ported from lib/ai/hybrid-router.ts

const SENSITIVITY_KEYWORDS: &[&str] = &[
    "quantization",
    "api",
    "finetuning",
    "offline ai",
    "edgeai",
    "cloud computing",
    "crypto",
    "blockchain",
    "legal",
    "privacy",
    "deep research",
];

/// Check if text contains sensitive topics requiring higher-tier models.
pub fn is_sensitive(text: &str) -> bool {
    let lower = text.to_lowercase();
    SENSITIVITY_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_crypto() {
        assert!(is_sensitive("Tell me about blockchain development"));
    }

    #[test]
    fn test_sensitive_privacy() {
        assert!(is_sensitive("What are the privacy implications?"));
    }

    #[test]
    fn test_not_sensitive() {
        assert!(!is_sensitive("How do I build a feed in Rust?"));
    }
}
