# Yaatal AI Harness

A modular Rust runtime for retrieval, ranking, recommendation, and agentic AI pipelines. Inspired by and building upon patterns from [zeroclaw-labs/zeroclaw](https://github.com/zeroclaw-labs/zeroclaw).

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        YAATAL HARNESS                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐   │
│  │  yaatal-core  │  Shared domain types and traits                 │   │
│  │               │  • RequestContext, Candidate, ScoredCandidate   │   │
│  │               │  • Retriever, Ranker, PolicyEngine traits        │   │
│  │               │  • LlmProvider, Tool, MemoryStore traits       │   │
│  │               │  • Observer for pipeline instrumentation       │   │
│  └───────────────┴────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │
│  │ yaatal-search │  │ yaatal-feed  │  │ yaatal-voice │                 │
│  │  Retrieval →  │  │  Candidate   │  │  Voice/      │                 │
│  │  Ranking →    │  │  generation  │  │  Agentic     │                 │
│  │  Policy       │  │  → Ranking → │  │  pipeline    │                 │
│  └──────────────┘  │  Policy      │  └──────────────┘                 │
│                    └──────────────┘                                    │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │
│  │ yaatal-models│  │ yaatal-tools │  │ yaatal-evals │                 │
│  │  OpenAI      │  │  Tool        │  │  MRR/NDCG    │                 │
│  │  Anthropic   │  │  Executor    │  │  metrics     │                 │
│  │  Fallback    │  │  + Builtins  │  │              │                 │
│  └──────────────┘  └──────────────┘  └──────────────┘                 │
│                                                                         │
│  ┌──────────────┐  ┌──────────────┐                                   │
│  │ yaatal-policy│  │ yaatal-observability│                              │
│  │  AllowAll    │  │  TracingObserver   │                              │
│  │  SimpleFilter│  │  Custom observers   │                              │
│  └──────────────┘  └──────────────┘                                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Crates

### yaatal-core

The foundational crate containing all shared types and traits.

```rust
use yaatal_core::{
    RequestContext, Candidate, ScoredCandidate,
    Retriever, Ranker, PolicyEngine,
    LlmProvider, Tool, MemoryStore, Observer,
    Message, ChatParams, LlmResponse,
};

// Create request context
let ctx = RequestContext::new("request-123");

// LLM chat completion
let messages = vec![
    Message::system("You are a helpful assistant."),
    Message::user("Hello!"),
];
let params = ChatParams {
    max_tokens: Some(1024),
    temperature: Some(0.7),
    ..Default::default()
};
```

### yaatal-models

LLM provider implementations and model adapters.

```rust
use yaatal_models::{OpenAiProvider, AnthropicProvider, FallbackRouter};
use yaatal_core::{LlmProvider, RequestContext, Message, ChatParams};
use std::sync::Arc;

// OpenAI provider
let openai = Arc::new(OpenAiProvider::new("sk-..."));

// Anthropic provider
let anthropic = Arc::new(AnthropicProvider::new("sk-ant-..."));

// Fallback router (tries providers in order)
let router = FallbackRouter::new(vec![openai, anthropic]);

// Chat completion
let ctx = RequestContext::new("req-1");
let response = router.chat(&ctx, &messages, ChatParams::default()).await?;
```

### yaatal-tools

Tool execution runtime with built-in tools. Only `file_read` and `session_note` build by
default (`default = ["safe-tools"]`); the dangerous built-ins below are compiled out unless
their Cargo feature is enabled, and `register_builtin` returns
`Err(ToolError::PermissionDenied)` naming the required feature when one is requested
without it.

```rust
use yaatal_tools::{ToolExecutor, BuiltinTool};
use yaatal_core::{RequestContext, Tool, ToolResult};

// Create executor with built-in tools
let executor = ToolExecutor::new();
executor.register_builtin(BuiltinTool::FileRead).await?;
// Requires the `local-shell` / `web-search` Cargo features (or `r-and-d-tools`):
executor.register_builtin(BuiltinTool::Shell).await?;
executor.register_builtin(BuiltinTool::Search).await?;

// Execute a tool
let ctx = RequestContext::new("req-1");
let result = executor.execute(&ctx, "shell", r#"{"command": "ls -la"}"#).await?;
```

#### Built-in Tools

| Tool | Description | Parameters | Cargo feature | Default? |
|------|-------------|------------|----------------|----------|
| `file_read` | Read file contents | `path: string`, `limit?: number` | `file-read` | yes |
| `session_note` | Persist/read session notes | `operation: string`, `content?: string` | `session-note` | yes |
| `shell` | Execute shell commands | `command: string`, `cwd?: string` | `local-shell` | no |
| `file_write` | Write to a file | `path: string`, `content: string` | `file-write` | no |
| `git` | Execute git commands | `command: string`, `cwd?: string` | `git` | no |
| `web_fetch` | Fetch URL content | `url: string`, `method?: string` | `web-fetch` (needs `network`) | no |
| `search` | Web search via DuckDuckGo | `query: string`, `num_results?: number` | `web-search` (needs `network`) | no |

### yaatal-search

Search pipeline implementation.

```rust
use yaatal_search::pipeline;

// Pipeline: retrieve → rank → policy
let result = pipeline(&ctx, &retriever, &ranker, &policy, "query").await?;
```

### yaatal-feed

Recommendation/feed pipeline.

```rust
use yaatal_feed::pipeline;

let result = pipeline(&ctx, &retriever, &ranker, &policy, "user-123").await?;
```

### yaatal-policy

Policy engine implementations.

```rust
use yaatal_policy::{AllowAllPolicy, SimpleFilterPolicy};

// Allow all items (for testing)
let policy = AllowAllPolicy;

// Filter blocked content
let policy = SimpleFilterPolicy;
```

### yaatal-evals

Evaluation metrics.

```rust
use yaatal_evals::{mean_reciprocal_rank, ndcg_at_k};

// Compute MRR
let mrr = mean_reciprocal_rank(&rankings, &relevant_ids);

// Compute NDCG@10
let ndcg = ndcg_at_k(&rankings, &relevant_ids, 10);
```

### yaatal-observability

Tracing and observability setup.

```rust
use yaatal_observability::init_tracing;

init_tracing();
```

## Traits Reference

### LlmProvider

Unified interface for LLM backends.

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn default_model(&self) -> &str;
    fn available_models(&self) -> Vec<String>;

    async fn chat(
        &self,
        ctx: &RequestContext,
        messages: &[Message],
        params: ChatParams,
    ) -> Result<LlmResponse, LlmError>;

    async fn embed(
        &self,
        ctx: &RequestContext,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, LlmError>;

    async fn health_check(&self) -> bool;
}
```

### Tool

Trait for executable tools.

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn metadata(&self) -> &ToolMetadata;

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError>;
}
```

### MemoryStore

Trait for memory storage backends.

```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn store(&self, entry: MemoryEntry) -> Result<String, MemoryError>;
    async fn recall(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError>;
    async fn search(&self, query: &str, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError>;
    async fn recent(&self, memory_type: Option<MemoryType>, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError>;
    async fn forget(&self, id: &str) -> Result<(), MemoryError>;
    async fn update(&self, entry: MemoryEntry) -> Result<(), MemoryError>;
}
```

### Observer

Trait for pipeline instrumentation.

```rust
#[async_trait]
pub trait Observer: Send + Sync {
    async fn on_event(&self, event: PipelineEvent) -> Result<(), ObserverError>;
    fn on_log(&self, level: &str, target: &str, message: &str);
}
```

## Pipeline Events

```rust
pub enum PipelineEvent {
    StageStarted { stage_name: String, request_id: String },
    StageCompleted { stage_name: String, request_id: String, duration_ms: u64 },
    StageFailed { stage_name: String, request_id: String, error: String },
    ToolStarted { tool_name: String, request_id: String },
    ToolCompleted { tool_name: String, request_id: String, duration_ms: u64, success: bool },
    LlmCallStarted { provider: String, model: String, request_id: String },
    LlmCallCompleted { provider: String, model: String, request_id: String, duration_ms: u64, tokens_used: u32 },
    LlmCallFailed { provider: String, model: String, request_id: String, error: String },
}
```

## Building

```bash
cargo build --workspace
```

## Testing

```bash
cargo test --workspace
```

## Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `tokio` | 1.x | Async runtime |
| `reqwest` | 0.12 | HTTP client |
| `serde` | 1.x | Serialization |
| `tracing` | 0.1 | Structured logging |
| `thiserror` | 1.x | Error handling |
| `async-trait` | 0.1 | Async traits |
| `chrono` | 0.4 | Date/time |
| `uuid` | 1.x | UUID generation |

## License

MIT OR Apache-2.0
