use regex::Regex;
use std::sync::LazyLock;

// All regex patterns below are compile-time constants — expect() is safe.
#[allow(clippy::expect_used)]
static SCRIPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<script[^>]*>[\s\S]*?</script>").expect("SCRIPT_RE regex"));
#[allow(clippy::expect_used)]
static ON_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)on\w+\s*=\s*(?:"[^"]*"|'[^']*'|\S+)"#).expect("ON_ATTR_RE regex")
});
#[allow(clippy::expect_used)]
static JS_URI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)javascript:").expect("JS_URI_RE regex"));
#[allow(clippy::expect_used)]
static DANGEROUS_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches dangerous HTML tags in two forms:
    // 1. Paired tags: <iframe>...</iframe>
    // 2. Self-closing or unclosed tags: <iframe /> or <iframe>
    Regex::new(r"(?i)<(iframe|object|embed|form|base|meta)[^>]*>[\s\S]*?</(iframe|object|embed|form|base|meta)>|<(iframe|object|embed|form|base|meta)[^>]*/?>")
        .expect("DANGEROUS_TAGS_RE regex")
});
#[allow(clippy::expect_used)]
static DATA_URI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)data:text/html").expect("DATA_URI_RE regex"));

pub fn sanitize_content(content: &str) -> String {
    let no_script = SCRIPT_RE.replace_all(content, "");
    let no_dangerous_tags = DANGEROUS_TAGS_RE.replace_all(&no_script, "");
    let no_attrs = ON_ATTR_RE.replace_all(&no_dangerous_tags, "");
    let no_js_uri = JS_URI_RE.replace_all(&no_attrs, "");
    let safe = DATA_URI_RE.replace_all(&no_js_uri, "");
    safe.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_script_tags() {
        let input = "Hello <script>alert('xss')</script> World";
        assert_eq!(sanitize_content(input), "Hello  World");
    }

    #[test]
    fn test_remove_event_handlers() {
        let input = r#"<div onclick="evil()">Safe content</div>"#;
        assert!(!sanitize_content(input).contains("onclick"));
    }

    #[test]
    fn test_remove_javascript_uri() {
        let input = "Click <a href='javascript:alert(1)'>here</a>";
        assert!(!sanitize_content(input).contains("javascript:"));
    }

    #[test]
    fn test_clean_content_unchanged() {
        let input = "Perfectly safe content with <b>bold</b> text.";
        assert_eq!(sanitize_content(input), input);
    }

    #[test]
    fn test_unquoted_event_handler() {
        let input = "<img src=x onerror=alert(1)>";
        let result = sanitize_content(input);
        assert!(!result.contains("onerror"));
        assert!(!result.contains("alert"));
    }

    #[test]
    fn test_dangerous_iframe() {
        let input = r#"<iframe src="evil.com"></iframe>"#;
        let result = sanitize_content(input);
        assert!(!result.contains("iframe"));
        assert!(!result.contains("evil.com"));
    }

    #[test]
    fn test_dangerous_object() {
        let input = r#"<object data="evil.swf"></object>"#;
        let result = sanitize_content(input);
        assert!(!result.contains("object"));
    }

    #[test]
    fn test_dangerous_embed() {
        let input = r#"<embed src="evil.swf">"#;
        let result = sanitize_content(input);
        assert!(!result.contains("embed"));
    }

    #[test]
    fn test_data_uri() {
        let input = r#"<a href="data:text/html,<script>alert(1)</script>">click</a>"#;
        let result = sanitize_content(input);
        assert!(!result.contains("data:text/html"));
    }
}
