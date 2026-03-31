//! Task classification for AI routing
//! Ported from lib/ai/hybrid-router.ts

use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum AiTask {
    Retrieval,
    Translate,
    Summarize,
    Chat,
    Create,
}

impl AiTask {
    /// Timeout per task type — African latency aware
    pub fn timeout(&self) -> Duration {
        match self {
            AiTask::Retrieval => Duration::from_millis(1500),
            AiTask::Translate => Duration::from_millis(2000),
            AiTask::Summarize => Duration::from_millis(2000),
            AiTask::Chat => Duration::from_millis(2000),
            AiTask::Create => Duration::from_millis(3000),
        }
    }
}

/// Classify user input into an AI task category.
/// Returns (task, reason) for logging.
pub fn classify_task(text: &str) -> (AiTask, &'static str) {
    let lower = text.to_lowercase();

    if lower.contains("summarize") || lower.contains("tldr") || lower.contains("résumé") {
        (AiTask::Summarize, "Keyword matched summarize")
    } else if lower.contains("translate") || lower.contains("traduire") {
        (AiTask::Translate, "Keyword matched translate")
    } else if lower.contains("retriev") || lower.contains("search") || lower.contains("cherch") {
        (AiTask::Retrieval, "Keyword matched retrieval")
    } else if lower.contains("create") || lower.contains("generate") || lower.contains("créer") {
        (AiTask::Create, "Keyword matched create")
    } else {
        (AiTask::Chat, "Default to chat")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_summarize() {
        let (task, _) = classify_task("Can you summarize this article?");
        assert_eq!(task, AiTask::Summarize);
    }

    #[test]
    fn test_classify_translate() {
        let (task, _) = classify_task("Translate this to French");
        assert_eq!(task, AiTask::Translate);
    }

    #[test]
    fn test_classify_french_keywords() {
        let (task, _) = classify_task("Peux-tu traduire ce texte?");
        assert_eq!(task, AiTask::Translate);
    }

    #[test]
    fn test_classify_retrieval() {
        let (task, _) = classify_task("Search for Rust tutorials in Africa");
        assert_eq!(task, AiTask::Retrieval);
    }

    #[test]
    fn test_classify_create() {
        let (task, _) = classify_task("Generate a logo for my startup");
        assert_eq!(task, AiTask::Create);
    }

    #[test]
    fn test_classify_default_chat() {
        let (task, _) = classify_task("Hello, how are you?");
        assert_eq!(task, AiTask::Chat);
    }

    #[test]
    fn test_task_timeouts() {
        assert_eq!(
            AiTask::Retrieval.timeout(),
            std::time::Duration::from_millis(1500)
        );
        assert_eq!(
            AiTask::Create.timeout(),
            std::time::Duration::from_millis(3000)
        );
    }
}
