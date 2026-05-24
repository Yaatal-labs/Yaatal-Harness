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
///
/// Single-token keywords are matched on word boundaries (so `"capability"`,
/// `"rapid"`, `"apiary"` do NOT match `"api"`). Multi-token keywords (containing
/// whitespace) are matched as substrings since they already carry boundary
/// context. Lane 0 follow-up: restores the Regex-equivalent guard that
/// yokk-engine dropped by replacing it with a flat substring match.
pub fn is_sensitive(text: &str) -> bool {
    let lower = text.to_lowercase();
    SENSITIVITY_KEYWORDS
        .iter()
        .any(|kw| contains_keyword(&lower, kw))
}

fn contains_keyword(haystack: &str, needle: &str) -> bool {
    if needle.contains(' ') {
        // Multi-token keywords already carry boundary context.
        return haystack.contains(needle);
    }
    // Single-token: split on non-alphanumeric and compare whole words.
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == needle)
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

    #[test]
    fn test_api_matches_word() {
        assert!(is_sensitive("Tell me about the OpenAI API"));
        assert!(is_sensitive("api keys should be rotated"));
        assert!(is_sensitive("Send a POST to the /api endpoint"));
    }

    #[test]
    fn test_api_false_positive() {
        // None of these contain the standalone word "api" — substring match
        // would have wrongly flagged them. Word-boundary match must not.
        assert!(!is_sensitive("This capability is important"));
        assert!(!is_sensitive("a rapid response"));
        assert!(!is_sensitive("the apiary has many bees"));
        assert!(!is_sensitive("therapist visit"));
        assert!(!is_sensitive("Napier's constant"));
    }

    #[test]
    fn test_multi_word_keyword_still_matches() {
        assert!(is_sensitive("Tell me about offline ai deployment"));
        assert!(is_sensitive("cloud computing costs are high"));
        assert!(is_sensitive("a deep research project"));
    }
}
