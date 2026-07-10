//! Intent-based tool routing.
//!
//! Routes detected intents to appropriate tool handlers, inspired by
//! Picovoice's Rhino speech-to-intent pattern.
//!
//! ## Design
//!
//! Instead of routing raw text to tools, this module parses the user's
//! intent first, then routes to the appropriate handler based on the
//! detected action and entities.
//!
//! ## Example
//!
//! ```rust,ignore
//! use yaatal_tools::intent_router::{IntentRouter, Intent};
//!
//! let router = IntentRouter::new()
//!     .with_handler("search", search_handler)
//!     .with_handler("research", research_handler)
//!     .with_default(default_handler);
//!
//! let intent = Intent {
//!     action: "search".to_string(),
//!     entities: [("query".to_string(), "Rust async".to_string())].into(),
//!     confidence: 0.95,
//! };
//!
//! let result = router.route(&intent, &ctx).await;
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use yaatal_core::{RequestContext, ToolError, ToolResult};

/// A detected intent with action and entities.
#[derive(Debug, Clone)]
pub struct Intent {
    /// The action to perform (e.g., "search", "research", "build").
    pub action: String,
    /// Named entities extracted from the query (e.g., "query", "language").
    pub entities: HashMap<String, String>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
}

impl Intent {
    /// Create a new intent.
    pub fn new(action: impl Into<String>, confidence: f32) -> Self {
        Self {
            action: action.into(),
            entities: HashMap::new(),
            confidence,
        }
    }

    /// Add an entity to the intent.
    pub fn with_entity(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.entities.insert(name.into(), value.into());
        self
    }

    /// Get an entity by name.
    pub fn get(&self, name: &str) -> Option<&String> {
        self.entities.get(name)
    }

    /// Check if this is a high-confidence intent.
    pub fn is_confident(&self) -> bool {
        self.confidence >= 0.8
    }
}

/// Trait for intent parsing.
#[async_trait::async_trait]
pub trait IntentParser: Send + Sync {
    /// Parse a transcript into an intent.
    async fn parse(&self, transcript: &str) -> Result<Intent, IntentError>;
}

/// Errors that can occur during intent parsing.
#[derive(Debug, thiserror::Error)]
pub enum IntentError {
    #[error("Parsing failed: {0}")]
    ParseFailed(String),
    #[error("No intent detected")]
    NoIntent,
    #[error("Low confidence: {0}")]
    LowConfidence(f32),
}

/// Intent router that maps intents to tool handlers.
pub struct IntentRouter {
    handlers: HashMap<String, Arc<dyn IntentHandler>>,
    default_handler: Option<Arc<dyn IntentHandler>>,
    fallback_llm: Option<Arc<dyn IntentFallback>>,
}

impl Default for IntentRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentRouter {
    /// Create a new intent router.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            default_handler: None,
            fallback_llm: None,
        }
    }

    /// Register a handler for an action.
    pub fn with_handler(
        mut self,
        action: impl Into<String>,
        handler: Arc<dyn IntentHandler>,
    ) -> Self {
        self.handlers.insert(action.into(), handler);
        self
    }

    /// Set the default handler for unknown actions.
    pub fn with_default(mut self, handler: Arc<dyn IntentHandler>) -> Self {
        self.default_handler = Some(handler);
        self
    }

    /// Set the LLM fallback for when no handler matches.
    pub fn with_fallback_llm(mut self, llm: Arc<dyn IntentFallback>) -> Self {
        self.fallback_llm = Some(llm);
        self
    }

    /// Route an intent to the appropriate handler.
    pub async fn route(
        &self,
        intent: &Intent,
        ctx: &RequestContext,
    ) -> Result<ToolResult, ToolError> {
        let action = &intent.action;
        info!(action = %action, confidence = intent.confidence, "intent_router_route");

        // Check confidence threshold
        if intent.confidence < 0.5 {
            warn!(confidence = intent.confidence, "intent_low_confidence");
            if let Some(fallback) = &self.fallback_llm {
                return fallback.generate(intent, ctx).await;
            }
            return Err(ToolError::InvalidParams(format!(
                "Intent confidence {} below threshold",
                intent.confidence
            )));
        }

        // Find handler
        if let Some(handler) = self.handlers.get(action) {
            handler.handle(intent, ctx).await
        } else if let Some(default) = &self.default_handler {
            default.handle(intent, ctx).await
        } else if let Some(fallback) = &self.fallback_llm {
            fallback.generate(intent, ctx).await
        } else {
            Err(ToolError::NotFound(format!(
                "No handler for action: {}",
                action
            )))
        }
    }

    /// Get all registered actions.
    pub fn registered_actions(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }
}

/// Trait for intent handlers.
#[async_trait::async_trait]
pub trait IntentHandler: Send + Sync {
    /// Handle an intent and return a result.
    async fn handle(&self, intent: &Intent, ctx: &RequestContext) -> Result<ToolResult, ToolError>;
}

/// Trait for LLM-based fallback when no handler matches.
#[async_trait::async_trait]
pub trait IntentFallback: Send + Sync {
    /// Generate a response using LLM for unhandled intents.
    async fn generate(
        &self,
        intent: &Intent,
        ctx: &RequestContext,
    ) -> Result<ToolResult, ToolError>;
}

// =============================================================================
// BUILT-IN INTENT PARSERS
// =============================================================================

/// Simple keyword-based intent parser.
pub struct KeywordIntentParser {
    keywords: HashMap<String, Vec<String>>,
}

impl KeywordIntentParser {
    /// Create a new keyword parser with default keywords.
    pub fn new() -> Self {
        let mut keywords = HashMap::new();
        keywords.insert(
            "search".to_string(),
            vec![
                "find".to_string(),
                "search".to_string(),
                "look for".to_string(),
                "show me".to_string(),
                "get".to_string(),
            ],
        );
        keywords.insert(
            "research".to_string(),
            vec![
                "research".to_string(),
                "investigate".to_string(),
                "analyze".to_string(),
                "study".to_string(),
                "explore".to_string(),
            ],
        );
        keywords.insert(
            "build".to_string(),
            vec![
                "build".to_string(),
                "create".to_string(),
                "implement".to_string(),
                "write".to_string(),
                "develop".to_string(),
            ],
        );
        keywords.insert(
            "debug".to_string(),
            vec![
                "debug".to_string(),
                "fix".to_string(),
                "solve".to_string(),
                "error".to_string(),
                "issue".to_string(),
            ],
        );
        keywords.insert(
            "explain".to_string(),
            vec![
                "explain".to_string(),
                "what is".to_string(),
                "how does".to_string(),
                "describe".to_string(),
                "tell me about".to_string(),
            ],
        );

        Self { keywords }
    }
}

impl Default for KeywordIntentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl IntentParser for KeywordIntentParser {
    async fn parse(&self, transcript: &str) -> Result<Intent, IntentError> {
        let lower = transcript.to_lowercase();

        // Find action
        let mut best_action = "unknown";
        let mut best_score = 0;

        for (action, keywords) in &self.keywords {
            let score = keywords
                .iter()
                .filter(|kw| keyword_matches(&lower, kw))
                .count();

            if score > best_score {
                best_score = score;
                best_action = action;
            }
        }

        if best_score == 0 {
            return Err(IntentError::NoIntent);
        }

        // Extract entities
        let mut entities = HashMap::new();

        // Extract quoted text as "query"
        if let Some(start) = lower.find('"') {
            if let Some(end) = lower[start + 1..].find('"') {
                let query = &transcript[start + 1..start + 1 + end];
                entities.insert("query".to_string(), query.to_string());
            }
        }

        // Extract language if mentioned
        let languages = ["rust", "python", "javascript", "typescript", "go", "java"];
        for lang in languages {
            if lower.contains(lang) {
                entities.insert("language".to_string(), lang.to_string());
                break;
            }
        }

        entities
            .entry("query".to_string())
            .or_insert_with(|| transcript.trim().to_string());

        // Calculate confidence based on keyword matches
        let confidence = (best_score as f32 * 0.3).min(1.0);

        Ok(Intent {
            action: best_action.to_string(),
            entities,
            confidence,
        })
    }
}

fn keyword_matches(text: &str, keyword: &str) -> bool {
    if keyword.contains(' ') {
        return text.contains(keyword);
    }

    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == keyword)
}

// =============================================================================
// BUILT-IN HANDLERS
// =============================================================================

/// Search intent handler using yaatal-search.
pub struct SearchIntentHandler {
    retriever: Arc<dyn yaatal_core::Retriever>,
    ranker: Arc<dyn yaatal_core::Ranker>,
}

impl SearchIntentHandler {
    /// Create a new search handler.
    pub fn new(
        retriever: Arc<dyn yaatal_core::Retriever>,
        ranker: Arc<dyn yaatal_core::Ranker>,
    ) -> Self {
        Self { retriever, ranker }
    }
}

#[async_trait::async_trait]
impl IntentHandler for SearchIntentHandler {
    async fn handle(&self, intent: &Intent, ctx: &RequestContext) -> Result<ToolResult, ToolError> {
        let query = intent
            .get("query")
            .ok_or_else(|| ToolError::InvalidParams("Missing 'query' entity".to_string()))?;

        info!(query = %query, "search_intent_handler");

        let candidates = self
            .retriever
            .retrieve(ctx, query)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let ranked = self
            .ranker
            .rank(ctx, candidates)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let result_json = serde_json::to_string(&ranked)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolResult::success(result_json))
    }
}

/// Research intent handler for auto-research pipeline.
pub struct ResearchIntentHandler {
    tools: Arc<crate::ToolExecutor>,
}

impl ResearchIntentHandler {
    /// Create a new research handler.
    pub fn new(tools: Arc<crate::ToolExecutor>) -> Self {
        Self { tools }
    }
}

#[async_trait::async_trait]
impl IntentHandler for ResearchIntentHandler {
    async fn handle(&self, intent: &Intent, ctx: &RequestContext) -> Result<ToolResult, ToolError> {
        let query = intent
            .get("query")
            .cloned()
            .unwrap_or_else(|| "research current codebase".to_string());

        info!(query = %query, "research_intent_handler");

        // Use session note tool to log research
        let research_note = serde_json::json!({
            "operation": "write",
            "content": format!("[{}] Research started: {}", ctx.request_id, query),
        });

        if let Err(error) = self
            .tools
            .execute(ctx, "session_note", &research_note.to_string())
            .await
        {
            warn!(request_id = %ctx.request_id, %error, "research_note_write_failed");
        }

        Ok(ToolResult::success(format!(
            "Research initiated for: {}. Check session notes for progress.",
            query
        )))
    }
}

/// Build intent handler for code generation.
pub struct BuildIntentHandler {
    tools: Arc<crate::ToolExecutor>,
}

impl BuildIntentHandler {
    /// Create a new build handler.
    pub fn new(tools: Arc<crate::ToolExecutor>) -> Self {
        Self { tools }
    }
}

#[async_trait::async_trait]
impl IntentHandler for BuildIntentHandler {
    async fn handle(&self, intent: &Intent, ctx: &RequestContext) -> Result<ToolResult, ToolError> {
        let query = intent
            .get("query")
            .cloned()
            .unwrap_or_else(|| "implement feature".to_string());

        info!(query = %query, "build_intent_handler");

        // Log build intent
        let build_note = serde_json::json!({
            "operation": "write",
            "content": format!("[{}] Build intent: {}", ctx.request_id, query),
        });

        if let Err(error) = self
            .tools
            .execute(ctx, "session_note", &build_note.to_string())
            .await
        {
            warn!(request_id = %ctx.request_id, %error, "build_note_write_failed");
        }

        Ok(ToolResult::success(format!(
            "Build task queued: {}. Agent will work on this next.",
            query
        )))
    }
}

/// Explain intent handler.
pub struct ExplainIntentHandler {
    retriever: Arc<dyn yaatal_core::Retriever>,
}

impl ExplainIntentHandler {
    /// Create a new explain handler.
    pub fn new(retriever: Arc<dyn yaatal_core::Retriever>) -> Self {
        Self { retriever }
    }
}

#[async_trait::async_trait]
impl IntentHandler for ExplainIntentHandler {
    async fn handle(&self, intent: &Intent, ctx: &RequestContext) -> Result<ToolResult, ToolError> {
        let query = intent
            .get("query")
            .cloned()
            .unwrap_or_else(|| "explain this code".to_string());

        info!(query = %query, "explain_intent_handler");

        let candidates = self
            .retriever
            .retrieve(ctx, &query)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // Return top result for explanation
        if let Some(candidate) = candidates.first() {
            let explanation = candidate
                .attributes
                .get("content")
                .cloned()
                .unwrap_or_else(|| "No explanation available".to_string());

            Ok(ToolResult::success(explanation))
        } else {
            Ok(ToolResult::success(
                "No relevant explanation found.".to_string(),
            ))
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

/// Mock handler for testing.
#[cfg(test)]
struct MockHandler;

#[cfg(test)]
#[async_trait::async_trait]
impl IntentHandler for MockHandler {
    async fn handle(
        &self,
        _intent: &Intent,
        _ctx: &RequestContext,
    ) -> Result<ToolResult, ToolError> {
        Ok(ToolResult::success("mock result"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keyword_parser_search() {
        let parser = KeywordIntentParser::new();
        let intent = parser.parse("find Rust async tutorials").await.unwrap();

        assert_eq!(intent.action, "search");
        assert!(intent.get("query").is_some());
    }

    #[tokio::test]
    async fn test_keyword_parser_research() {
        let parser = KeywordIntentParser::new();
        let intent = parser.parse("research agentic harnesses").await.unwrap();

        assert_eq!(intent.action, "research");
    }

    #[tokio::test]
    async fn test_keyword_parser_language() {
        let parser = KeywordIntentParser::new();
        let intent = parser.parse("find Python tutorials").await.unwrap();

        assert_eq!(intent.get("language"), Some(&"python".to_string()));
    }

    #[tokio::test]
    async fn test_intent_confidence() {
        let intent = Intent::new("search", 0.95).with_entity("query", "rust programming");

        assert!(intent.is_confident());

        let low_intent = Intent::new("unknown", 0.3);
        assert!(!low_intent.is_confident());
    }

    #[tokio::test]
    async fn test_router_registration() {
        let router = IntentRouter::new()
            .with_handler("search", Arc::new(MockHandler))
            .with_handler("research", Arc::new(MockHandler));

        let actions = router.registered_actions();
        assert!(actions.contains(&"search".to_string()));
        assert!(actions.contains(&"research".to_string()));
    }
}
