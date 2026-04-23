//! Core domain types and traits used throughout the Yaatal AI harness.
//!
//! This crate defines the canonical data structures and trait definitions
//! that the rest of the system builds upon. By keeping these in a separate
//! crate, we avoid cyclic dependencies and keep a clear separation
//! between the runtime kernel, pipeline orchestrators and model adapters.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Context carried with every request through the pipeline. Contains
/// metadata such as tenant information, trace identifiers and deadlines.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    /// Unique request identifier for tracing and debugging.
    pub request_id: String,
    /// The tenant or user making the request. Optional for unauthenticated queries.
    pub tenant: Option<String>,
    /// Arbitrary key/value pairs for storing additional request metadata.
    pub metadata: HashMap<String, String>,
}

impl RequestContext {
    /// Create a new `RequestContext` with a request id and empty metadata.
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            tenant: None,
            metadata: HashMap::new(),
        }
    }
}

/// Represents a candidate item returned from a retrieval stage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    /// The unique identifier of the item (e.g. document ID or user ID).
    pub id: String,
    /// A map of arbitrary attributes associated with the candidate.
    pub attributes: HashMap<String, String>,
}

/// A candidate with an attached score assigned by a ranking stage.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScoredCandidate {
    pub candidate: Candidate,
    pub score: f32,
    /// Optional metadata (e.g., reranking reason, model confidence).
    pub metadata: Option<HashMap<String, String>>,
}

/// The outcome of a policy evaluation step. Contains lists of items that
/// have been allowed or denied along with reasons.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyResult {
    pub allowed: Vec<ScoredCandidate>,
    pub denied: Vec<(ScoredCandidate, String)>,
}

/// Errors that may occur when running pipeline stages.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    /// A general IO or network error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// An error returned by an external service.
    #[error("External service error: {0}")]
    External(String),
    /// Any other error.
    #[error("Unexpected error: {0}")]
    Other(String),
}

/// Trait for retrieving a set of candidate items given a query context.
#[async_trait::async_trait]
pub trait Retriever: Send + Sync {
    /// Perform retrieval and return a list of candidates.
    async fn retrieve(
        &self,
        ctx: &RequestContext,
        query: &str,
    ) -> Result<Vec<Candidate>, HarnessError>;
}

/// Trait for ranking a set of candidate items.
#[async_trait::async_trait]
pub trait Ranker: Send + Sync {
    /// Given a context and a set of candidates, return scored candidates.
    async fn rank(
        &self,
        ctx: &RequestContext,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<ScoredCandidate>, HarnessError>;
}

/// Trait for enforcing policy on ranked items.
#[async_trait::async_trait]
pub trait PolicyEngine: Send + Sync {
    async fn evaluate(
        &self,
        ctx: &RequestContext,
        items: Vec<ScoredCandidate>,
    ) -> Result<PolicyResult, HarnessError>;
}

/// Trait for abstracting different model providers. Allows swapping out
/// local or remote models without coupling the pipeline to the provider.
#[async_trait::async_trait]
pub trait ModelAdapter<I, O>: Send + Sync {
    async fn infer(&self, ctx: &RequestContext, input: I) -> Result<O, HarnessError>;
}

/// A single step in a pipeline. Stages operate on some input and produce
/// an output that may feed into subsequent stages.
#[async_trait::async_trait]
pub trait PipelineStage {
    type Input;
    type Output;
    async fn run(
        &self,
        ctx: &RequestContext,
        input: Self::Input,
    ) -> Result<Self::Output, HarnessError>;
}

// =============================================================================
// ZEROCLAW-DERIVED TRAITS: Provider, Tool, Memory, Observer
// Extracted from zeroclaw-labs/zeroclaw for AI harness integration
// =============================================================================

/// Message role in a conversation context.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    ToolResult,
}

/// A single message in a conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    /// Optional name field for tool messages.
    pub name: Option<String>,
    /// Optional extended thinking/reasoning content.
    /// Based on Anthropic's extended thinking pattern.
    pub thinking: Option<String>,
}

impl Message {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            name: None,
            thinking: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            name: None,
            thinking: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            name: None,
            thinking: None,
        }
    }

    /// Create an assistant message with thinking.
    pub fn assistant_with_thinking(
        content: impl Into<String>,
        thinking: impl Into<String>,
    ) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            name: None,
            thinking: Some(thinking.into()),
        }
    }

    /// Create a tool result message.
    pub fn tool_result(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::ToolResult,
            content: content.into(),
            name: Some(name.into()),
            thinking: None,
        }
    }
}

/// Parameters for a chat completion request.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChatParams {
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0-2.0).
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter.
    pub top_p: Option<f32>,
    /// Stop sequences.
    pub stop: Option<Vec<String>>,
    /// JSON schema for structured output (optional).
    pub response_format: Option<String>,
}

/// A tool call returned from the model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    /// The name of the tool to call.
    pub name: String,
    /// The arguments as a JSON string.
    pub arguments: String,
}

/// The response from an LLM provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmResponse {
    /// The generated text content.
    pub content: String,
    /// Any tool calls the model requested.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage information (optional).
    pub usage: Option<TokenUsage>,
}

impl LlmResponse {
    /// Create a simple text response.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: vec![],
            usage: Some(TokenUsage::default()),
        }
    }

    /// Create a response with tool calls.
    pub fn with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content: content.into(),
            tool_calls,
            usage: Some(TokenUsage::default()),
        }
    }
}

/// Token usage statistics from an LLM call.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Errors specific to LLM operations.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Context length exceeded")]
    ContextLengthExceeded,
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),
}

/// Trait for LLM providers (extracted from zeroclaw).
/// Provides a unified interface for interacting with various LLM backends.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name (e.g., "openai", "anthropic").
    fn provider_name(&self) -> &str;

    /// Get the default model for this provider.
    fn default_model(&self) -> &str;

    /// List available models for this provider.
    fn available_models(&self) -> Vec<String>;

    /// Generate a chat completion.
    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        params: ChatParams,
    ) -> Result<LlmResponse, LlmError>;

    /// Generate embeddings for the given texts.
    async fn embed(
        &self,
        ctx: &RequestContext,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError>;

    /// Check if the provider is healthy.
    async fn health_check(&self) -> bool;
}

/// Builder for constructing LLM requests with fallback support.
pub struct LlmRequest {
    pub messages: Vec<Message>,
    pub params: ChatParams,
    pub model: Option<String>,
    /// Fallback models to try if the primary fails.
    pub fallback_models: Vec<String>,
}

impl LlmRequest {
    /// Create a new request with the given messages.
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            params: ChatParams::default(),
            model: None,
            fallback_models: vec![],
        }
    }

    /// Set the model to use.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set request parameters.
    pub fn params(mut self, params: ChatParams) -> Self {
        self.params = params;
        self
    }

    /// Add a fallback model.
    pub fn fallback(mut self, model: impl Into<String>) -> Self {
        self.fallback_models.push(model.into());
        self
    }
}

// =============================================================================
// TOOL SYSTEM (Extracted from zeroclaw-tools)
// =============================================================================

/// A parameter definition for a tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub param_type: ToolParamType,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolParamType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Object,
}

/// Metadata describing a tool's capabilities.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub parameters: Vec<ToolParameter>,
}

impl ToolMetadata {
    /// Create metadata for a tool.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            category: None,
            parameters: vec![],
        }
    }

    /// Add a required string parameter.
    pub fn param_string(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.parameters.push(ToolParameter {
            name: name.into(),
            description: description.into(),
            param_type: ToolParamType::String,
            required: true,
            default: None,
        });
        self
    }

    /// Add an optional string parameter with default.
    pub fn optional_string(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        default: impl Into<String>,
    ) -> Self {
        self.parameters.push(ToolParameter {
            name: name.into(),
            description: description.into(),
            param_type: ToolParamType::String,
            required: false,
            default: Some(default.into()),
        });
        self
    }
}

/// The result of executing a tool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully.
    pub success: bool,
    /// The output content from the tool.
    pub content: String,
    /// Error message if the tool failed.
    pub error: Option<String>,
    /// How long the execution took in milliseconds.
    pub execution_time_ms: u64,
}

impl ToolResult {
    /// Create a successful result.
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            error: None,
            execution_time_ms: 0,
        }
    }

    /// Create an error result.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            content: String::new(),
            error: Some(msg.into()),
            execution_time_ms: 0,
        }
    }
}

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
}

/// Trait for tools that can be invoked by the agent (extracted from zeroclaw).
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Get the metadata describing this tool.
    fn metadata(&self) -> &ToolMetadata;

    /// Execute the tool with the given JSON arguments.
    async fn execute(&self, ctx: &RequestContext, arguments: &str)
        -> Result<ToolResult, ToolError>;

    /// Validate that arguments are well-formed.
    fn validate_args(&self, arguments: &str) -> Result<(), ToolError> {
        // Default implementation: accept all arguments.
        // Override for strict validation.
        let _ = arguments;
        Ok(())
    }
}

// =============================================================================
// MEMORY SYSTEM (Extracted from zeroclaw-memory)
// =============================================================================

/// A memory entry stored in the memory system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for this memory.
    pub id: String,
    /// The memory content.
    pub content: String,
    /// Memory type/category.
    pub memory_type: MemoryType,
    /// When this memory was created.
    pub created_at: DateTime<Utc>,
    /// When this memory was last accessed.
    pub accessed_at: DateTime<Utc>,
    /// Arbitrary metadata.
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemoryType {
    /// A general fact or piece of information.
    Fact,
    /// A user preference or setting.
    Preference,
    /// A conversation or interaction summary.
    Conversation,
    /// Project-specific knowledge.
    Project,
    /// A skill or capability.
    Skill,
}

impl MemoryEntry {
    /// Create a new memory entry with generated ID.
    pub fn new(memory_type: impl Into<String>, content: impl Into<String>) -> Self {
        let memory_type = match memory_type.into().as_str() {
            "fact" => MemoryType::Fact,
            "preference" => MemoryType::Preference,
            "conversation" => MemoryType::Conversation,
            "project" => MemoryType::Project,
            "skill" => MemoryType::Skill,
            _ => MemoryType::Fact,
        };

        Self {
            id: Uuid::new_v4().to_string(),
            content: content.into(),
            memory_type,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create a project memory.
    pub fn project(content: impl Into<String>) -> Self {
        Self::new("project", content)
    }

    /// Create a fact memory.
    pub fn fact(content: impl Into<String>) -> Self {
        Self::new("fact", content)
    }

    /// Create a conversation memory.
    pub fn conversation(content: impl Into<String>) -> Self {
        Self::new("conversation", content)
    }
}

/// Errors that can occur during memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Memory not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Query error: {0}")]
    QueryError(String),
}

/// Trait for memory storage backends (extracted from zeroclaw).
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync {
    /// Store a new memory entry.
    async fn store(&self, entry: MemoryEntry) -> Result<String, MemoryError>;

    /// Retrieve a memory by ID.
    async fn recall(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError>;

    /// Search memories by content or metadata.
    async fn search(
        &self,
        query: &str,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// List recent memories, optionally filtered by type.
    async fn recent(
        &self,
        memory_type: Option<MemoryType>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// Delete a memory by ID.
    async fn forget(&self, id: &str) -> Result<(), MemoryError>;

    /// Update an existing memory.
    async fn update(&self, entry: MemoryEntry) -> Result<(), MemoryError>;
}

// =============================================================================
// OBSERVABILITY (Extracted from zeroclaw Observer)
// =============================================================================

/// Events that can be observed during pipeline execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PipelineEvent {
    /// Pipeline stage started.
    StageStarted {
        stage_name: String,
        request_id: String,
    },
    /// Pipeline stage completed.
    StageCompleted {
        stage_name: String,
        request_id: String,
        duration_ms: u64,
    },
    /// Pipeline stage failed.
    StageFailed {
        stage_name: String,
        request_id: String,
        error: String,
    },
    /// Tool execution started.
    ToolStarted {
        tool_name: String,
        request_id: String,
    },
    /// Tool execution completed.
    ToolCompleted {
        tool_name: String,
        request_id: String,
        duration_ms: u64,
        success: bool,
    },
    /// LLM call started.
    LlmCallStarted {
        provider: String,
        model: String,
        request_id: String,
    },
    /// LLM call completed.
    LlmCallCompleted {
        provider: String,
        model: String,
        request_id: String,
        duration_ms: u64,
        tokens_used: u32,
    },
    /// LLM call failed.
    LlmCallFailed {
        provider: String,
        model: String,
        request_id: String,
        error: String,
    },
}

/// Errors for observer operations.
#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Observer error: {0}")]
    Other(String),
}

/// Observer trait for pipeline instrumentation (extracted from zeroclaw).
/// Implement this trait to add custom observability to the harness.
#[async_trait::async_trait]
pub trait Observer: Send + Sync {
    /// Called when a pipeline event occurs.
    async fn on_event(&self, event: PipelineEvent) -> Result<(), ObserverError>;

    /// Called when a span/log entry should be recorded.
    fn on_log(&self, level: &str, target: &str, message: &str) {
        let _ = (level, target, message);
    }
}

/// Basic logging observer that uses the tracing crate.
pub struct TracingObserver;

#[async_trait::async_trait]
impl Observer for TracingObserver {
    async fn on_event(&self, event: PipelineEvent) -> Result<(), ObserverError> {
        match event {
            PipelineEvent::StageStarted {
                stage_name,
                request_id,
            } => {
                tracing::info!(stage = %stage_name, request_id = %request_id, "Stage started");
            }
            PipelineEvent::StageCompleted {
                stage_name,
                request_id,
                duration_ms,
            } => {
                tracing::info!(stage = %stage_name, request_id = %request_id, duration_ms = %duration_ms, "Stage completed");
            }
            PipelineEvent::StageFailed {
                stage_name,
                request_id,
                error,
            } => {
                tracing::error!(stage = %stage_name, request_id = %request_id, error = %error, "Stage failed");
            }
            PipelineEvent::ToolStarted {
                tool_name,
                request_id,
            } => {
                tracing::info!(tool = %tool_name, request_id = %request_id, "Tool started");
            }
            PipelineEvent::ToolCompleted {
                tool_name,
                request_id,
                duration_ms,
                success,
            } => {
                tracing::info!(tool = %tool_name, request_id = %request_id, duration_ms = %duration_ms, success = %success, "Tool completed");
            }
            PipelineEvent::LlmCallStarted {
                provider,
                model,
                request_id,
            } => {
                tracing::info!(provider = %provider, model = %model, request_id = %request_id, "LLM call started");
            }
            PipelineEvent::LlmCallCompleted {
                provider,
                model,
                request_id,
                duration_ms,
                tokens_used,
            } => {
                tracing::info!(provider = %provider, model = %model, request_id = %request_id, duration_ms = %duration_ms, tokens = %tokens_used, "LLM call completed");
            }
            PipelineEvent::LlmCallFailed {
                provider,
                model,
                request_id,
                error,
            } => {
                tracing::error!(provider = %provider, model = %model, request_id = %request_id, error = %error, "LLM call failed");
            }
        }
        Ok(())
    }
}
