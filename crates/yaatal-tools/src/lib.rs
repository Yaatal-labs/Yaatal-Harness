//! Tool system for the Yaatal AI harness.
//!
//! This crate provides the tool execution infrastructure extracted from
//! zeroclaw-labs/zeroclaw. It includes:
//!
//! - [`ToolExecutor`]: Runtime for managing and executing tools
//! - Built-in tools: Shell, File I/O, Git, Web Fetch, Search, Session Notes
//! - Tool registry with validation
//! - Max steps limiting (prevents infinite loops)
//! - Session persistence for long-running agents
//! - [`intent_router`]: Intent-based tool routing (Picovoice pattern)
//!
//! ## Example
//!
//! ```rust
//! use yaatal_tools::{ToolExecutor, BuiltinTool};
//! use yaatal_core::{RequestContext, Tool, ToolResult};
//!
//! #[tokio::main]
//! async fn main() {
//!     let executor = ToolExecutor::new();
//!     executor.register_builtin(BuiltinTool::Shell).await;
//!
//!     let ctx = RequestContext::new("test");
//!     let result = executor.execute(&ctx, "shell", r#"{"command": "echo hello"}"#).await;
//!     println!("{:?}", result);
//! }
//! ```

pub mod intent_router;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::sync::RwLock;
use yaatal_core::{
    Observer, PipelineEvent, RequestContext, Tool, ToolError, ToolMetadata, ToolResult,
};

// =============================================================================
// TOOL EXECUTOR (Extracted from zeroclaw-tools)
// =============================================================================

/// Tool execution runtime that manages tool registration and execution.
pub struct ToolExecutor {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
    observers: RwLock<Vec<Arc<dyn Observer>>>,
    timeout_ms: u64,
    max_steps: usize,
    current_step: RwLock<usize>,
    /// Working directory for resolving relative paths
    workspace_dir: RwLock<String>,
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor {
    /// Create a new tool executor.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            observers: RwLock::new(Vec::new()),
            timeout_ms: 30_000, // 30 second default timeout
            max_steps: 100,     // Default max steps (prevents infinite loops)
            current_step: RwLock::new(0),
            workspace_dir: RwLock::new(
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string()),
            ),
        }
    }

    /// Set the default timeout for tool execution.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the maximum number of tool calls allowed (prevents infinite loops).
    /// Based on MiniMax's research: agents need stopping conditions.
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Set the working directory for resolving relative paths.
    /// Based on Anthropic's ACI principle: always use absolute paths.
    pub fn with_workspace_dir(mut self, dir: impl Into<String>) -> Self {
        self.workspace_dir = RwLock::new(dir.into());
        self
    }

    /// Get current step count.
    pub async fn current_step(&self) -> usize {
        *self.current_step.read().await
    }

    /// Check if max steps exceeded.
    pub async fn max_steps_exceeded(&self) -> bool {
        *self.current_step.read().await >= self.max_steps
    }

    /// Reset step counter (for new session).
    pub async fn reset_steps(&self) {
        *self.current_step.write().await = 0;
    }

    /// Get workspace directory.
    pub async fn workspace_dir(&self) -> String {
        self.workspace_dir.read().await.clone()
    }

    /// Resolve a path to absolute path, using workspace_dir as base.
    pub async fn resolve_path(&self, path: &str) -> String {
        if std::path::Path::is_absolute(std::path::Path::new(path)) {
            path.to_string()
        } else {
            let workspace = self.workspace_dir.read().await;
            let resolved = std::path::Path::new(&*workspace).join(path);
            resolved.to_string_lossy().to_string()
        }
    }

    /// Register a tool with the executor.
    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.metadata().name.clone();
        self.tools.write().await.insert(name, tool);
    }

    /// Register a builtin tool.
    pub async fn register_builtin(&self, tool: BuiltinTool) {
        let boxed: Arc<dyn Tool> = match tool {
            BuiltinTool::Shell => Arc::new(ShellTool::new()),
            BuiltinTool::FileRead => Arc::new(FileReadTool::new()),
            BuiltinTool::FileWrite => Arc::new(FileWriteTool::new()),
            BuiltinTool::Git => Arc::new(GitTool::new()),
            BuiltinTool::WebFetch => Arc::new(WebFetchTool::new()),
            BuiltinTool::Search => Arc::new(SearchTool::new()),
            BuiltinTool::SessionNote => Arc::new(SessionNoteTool::new()),
        };
        self.register(boxed).await;
    }

    /// Add an observer for tool execution events.
    pub async fn add_observer(&self, observer: Arc<dyn Observer>) {
        self.observers.write().await.push(observer);
    }

    /// List all registered tools.
    pub async fn list_tools(&self) -> Vec<ToolMetadata> {
        let tools = self.tools.read().await;
        tools.values().map(|t| t.metadata().clone()).collect()
    }

    /// Get metadata for a specific tool.
    pub async fn get_tool(&self, name: &str) -> Option<ToolMetadata> {
        let tools = self.tools.read().await;
        tools.get(name).map(|t| t.metadata().clone())
    }

    /// Execute a tool by name with the given arguments.
    pub async fn execute(
        &self,
        ctx: &RequestContext,
        name: &str,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        // Check max steps (prevents infinite loops - from research)
        {
            let current = *self.current_step.read().await;
            if current >= self.max_steps {
                return Err(ToolError::ExecutionFailed(format!(
                    "Max steps ({}) exceeded. Stopping to prevent infinite loop.",
                    self.max_steps
                )));
            }
        }

        let tools = self.tools.read().await;

        let tool = tools
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;

        // Emit tool started event
        self.emit_event(PipelineEvent::ToolStarted {
            tool_name: name.to_string(),
            request_id: ctx.request_id.clone(),
        })
        .await;

        let start = Instant::now();

        // Validate arguments
        tool.validate_args(arguments)?;

        // Execute with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(self.timeout_ms),
            tool.execute(ctx, arguments),
        )
        .await
        .map_err(|_| ToolError::Timeout(self.timeout_ms))?;

        let duration = start.elapsed().as_millis() as u64;

        let mut result = result?;

        // Update execution time
        result.execution_time_ms = duration;

        // Increment step counter (for max_steps tracking)
        *self.current_step.write().await += 1;

        // Emit tool completed event
        self.emit_event(PipelineEvent::ToolCompleted {
            tool_name: name.to_string(),
            request_id: ctx.request_id.clone(),
            duration_ms: duration,
            success: result.success,
        })
        .await;

        Ok(result)
    }

    /// Execute a tool and return the content directly.
    pub async fn execute_content(
        &self,
        ctx: &RequestContext,
        name: &str,
        arguments: &str,
    ) -> Result<String, ToolError> {
        let result = self.execute(ctx, name, arguments).await?;
        if result.success {
            Ok(result.content)
        } else {
            Err(ToolError::ExecutionFailed(result.error.unwrap_or_default()))
        }
    }

    async fn emit_event(&self, event: PipelineEvent) {
        let observers = self.observers.read().await;
        for observer in observers.iter() {
            if let Err(e) = observer.on_event(event.clone()).await {
                tracing::error!(error = %e, "Observer error");
            }
        }
    }
}

// =============================================================================
// BUILTIN TOOLS (Extracted from zeroclaw-tools)
// =============================================================================

/// Enum for built-in tool types.
#[derive(Debug, Clone, Copy)]
pub enum BuiltinTool {
    Shell,
    FileRead,
    FileWrite,
    Git,
    WebFetch,
    Search,
    /// Session note tool for persisting progress across sessions.
    /// Based on MiniMax mini-agent's SessionNoteTool.
    SessionNote,
}

/// Shell command execution tool.
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new("shell", "Execute a shell command and return the output")
                .param_string("command", "The shell command to execute")
                .optional_string("cwd", "Working directory for the command", ".")
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        use std::process::Command;

        #[derive(serde::Deserialize)]
        struct Args {
            command: String,
            #[serde(default)]
            cwd: Option<String>,
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        tracing::info!(shell_command = %args.command, request_id = %ctx.request_id);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&args.command);

        if let Some(cwd) = &args.cwd {
            cmd.current_dir(cwd);
        }

        let output = cmd
            .output()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(ToolResult::success(stdout))
        } else {
            Ok(ToolResult {
                success: false,
                content: stdout,
                error: Some(stderr),
                execution_time_ms: 0,
            })
        }
    }
}

/// File read tool.
pub struct FileReadTool {
    /// Base directory for resolving relative paths ( ACI principle: always absolute paths)
    base_dir: Option<String>,
}

impl FileReadTool {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Set base directory for resolving relative paths.
    pub fn with_base_dir(mut self, dir: impl Into<String>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    fn resolve_path(&self, path: &str) -> String {
        if std::path::Path::new(path).is_absolute() {
            path.to_string()
        } else if let Some(base) = &self.base_dir {
            std::path::Path::new(base)
                .join(path)
                .to_string_lossy()
                .to_string()
        } else {
            std::env::current_dir()
                .map(|p| p.join(path).to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string())
        }
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new(
            "file_read",
            "Read the contents of a file. Paths are resolved relative to workspace if relative.",
        )
        .param_string("path", "Path to the file to read (absolute or relative to workspace)")
        .optional_string("limit", "Maximum number of lines to read", "1000")
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            path: String,
            #[serde(default = "default_limit")]
            limit: usize,
        }

        fn default_limit() -> usize {
            1000
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Resolve path to absolute (ACI principle: prevent path confusion)
        let resolved_path = self.resolve_path(&args.path);

        tracing::info!(file_path = %resolved_path, request_id = %ctx.request_id);

        // Security check - prevent path traversal
        if resolved_path.contains("..") {
            return Err(ToolError::PermissionDenied(
                "Path traversal not allowed".to_string(),
            ));
        }

        let content = tokio::fs::read_to_string(&resolved_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let lines: Vec<&str> = content.lines().take(args.limit).collect();
        let result = lines.join("\n");

        Ok(ToolResult::success(result))
    }
}

/// File write tool.
pub struct FileWriteTool {
    /// Base directory for resolving relative paths
    base_dir: Option<String>,
}

impl FileWriteTool {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Set base directory for resolving relative paths.
    pub fn with_base_dir(mut self, dir: impl Into<String>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    fn resolve_path(&self, path: &str) -> String {
        if std::path::Path::new(path).is_absolute() {
            path.to_string()
        } else if let Some(base) = &self.base_dir {
            std::path::Path::new(base)
                .join(path)
                .to_string_lossy()
                .to_string()
        } else {
            std::env::current_dir()
                .map(|p| p.join(path).to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string())
        }
    }
}

impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> =
            LazyLock::new(|| {
                ToolMetadata::new(
            "file_write",
            "Write content to a file. Paths are resolved relative to workspace if relative.",
        )
        .param_string("path", "Path to the file to write (absolute or relative to workspace)")
        .param_string("content", "Content to write to the file")
            });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            path: String,
            content: String,
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Resolve path to absolute (ACI principle: prevent path confusion)
        let resolved_path = self.resolve_path(&args.path);

        tracing::info!(file_path = %resolved_path, request_id = %ctx.request_id);

        // Security check - prevent path traversal
        if resolved_path.contains("..") {
            return Err(ToolError::PermissionDenied(
                "Path traversal not allowed".to_string(),
            ));
        }

        tokio::fs::write(&resolved_path, &args.content)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolResult::success(format!(
            "Written {} bytes to {}",
            args.content.len(),
            resolved_path
        )))
    }
}

/// Git operations tool.
pub struct GitTool;

impl GitTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GitTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new("git", "Execute git commands")
                .param_string(
                    "command",
                    "The git command to execute (e.g., 'status', 'log --oneline -5')",
                )
                .optional_string("cwd", "Working directory for the command", ".")
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        use std::process::Command;

        #[derive(serde::Deserialize)]
        struct Args {
            command: String,
            #[serde(default = "default_cwd")]
            cwd: String,
        }

        fn default_cwd() -> String {
            ".".to_string()
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Security: only allow safe git commands
        let safe_commands = [
            "status", "log", "diff", "branch", "checkout", "pull", "fetch", "show",
        ];
        let cmd_name = args.command.split_whitespace().next().unwrap_or("");

        if !safe_commands.contains(&cmd_name) {
            return Err(ToolError::PermissionDenied(format!(
                "Git command '{}' is not allowed",
                cmd_name
            )));
        }

        tracing::info!(git_command = %args.command, request_id = %ctx.request_id);

        let mut cmd = Command::new("git");
        for arg in args.command.split_whitespace() {
            cmd.arg(arg);
        }
        cmd.current_dir(&args.cwd);

        let output = cmd
            .output()
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(ToolResult::success(stdout))
        } else {
            Ok(ToolResult {
                success: false,
                content: stdout,
                error: Some(stderr),
                execution_time_ms: 0,
            })
        }
    }
}

/// Web fetch tool.
pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new("web_fetch", "Fetch content from a URL")
                .param_string("url", "The URL to fetch")
                .optional_string("method", "HTTP method to use", "GET")
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            url: String,
            #[serde(default = "default_method")]
            method: String,
        }

        fn default_method() -> String {
            "GET".to_string()
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        // Basic URL validation
        if !args.url.starts_with("http://") && !args.url.starts_with("https://") {
            return Err(ToolError::InvalidParams(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        tracing::info!(url = %args.url, request_id = %ctx.request_id);

        let client = reqwest::Client::new();
        let response = match args.method.to_uppercase().as_str() {
            "GET" => client.get(&args.url),
            "POST" => client.post(&args.url),
            method => {
                return Err(ToolError::InvalidParams(format!(
                    "Unsupported HTTP method: {}",
                    method
                )));
            }
        }
        .send()
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            Ok(ToolResult::success(body))
        } else {
            Ok(ToolResult {
                success: false,
                content: body,
                error: Some(format!("HTTP {}", status)),
                execution_time_ms: 0,
            })
        }
    }
}

/// Web search tool (uses DuckDuckGo).
pub struct SearchTool;

impl SearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new("search", "Search the web using DuckDuckGo")
                .param_string("query", "The search query")
                .optional_string("num_results", "Number of results to return", "5")
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            query: String,
            #[serde(default = "default_num")]
            num_results: usize,
        }

        fn default_num() -> usize {
            5
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        tracing::info!(query = %args.query, request_id = %ctx.request_id);

        // Use DuckDuckGo HTML interface for simple search
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(&args.query)
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let body = response.text().await.unwrap_or_default();

        // Simple HTML parsing to extract results
        let mut results = Vec::new();
        for line in body.lines() {
            if line.contains("result__snippet") {
                // Extract snippet
                if let Some(start) = line.find(">") {
                    let snippet = &line[start + 1..];
                    if let Some(end) = snippet.find('<') {
                        results.push(snippet[..end].trim().to_string());
                    }
                }
            }
            if results.len() >= args.num_results {
                break;
            }
        }

        let output = if results.is_empty() {
            "No results found".to_string()
        } else {
            results
                .iter()
                .enumerate()
                .map(|(i, r)| format!("{}. {}", i + 1, r))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(ToolResult::success(output))
    }
}

// =============================================================================
// SESSION NOTE TOOL (From MiniMax mini-agent research)
// =============================================================================

/// Session note tool for persisting progress across sessions.
/// Based on MiniMax's mini-agent SessionNoteTool.
/// This enables long-running agents to maintain state between sessions.

/// In-memory session notes store.
/// In production, this could be backed by a file or database.
use tokio::sync::Mutex;

static SESSION_NOTES: LazyLock<Mutex<HashMap<String, Vec<SessionNoteEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionNoteEntry {
    timestamp: String,
    content: String,
}

/// Session note tool for reading/writing persistent notes.
/// This is critical for long-running agents that need to resume after crashes.
pub struct SessionNoteTool;

impl SessionNoteTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SessionNoteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SessionNoteTool {
    fn metadata(&self) -> &ToolMetadata {
        static METADATA: LazyLock<ToolMetadata> = LazyLock::new(|| {
            ToolMetadata::new(
                "session_note",
                "Read or write session notes for maintaining state across sessions",
            )
            .param_string("operation", "Operation to perform: 'read' or 'write'")
            .optional_string(
                "content",
                "Content to write (required for 'write' operation)",
                "",
            )
        });

        &METADATA
    }

    async fn execute(
        &self,
        ctx: &RequestContext,
        arguments: &str,
    ) -> Result<ToolResult, ToolError> {
        #[derive(serde::Deserialize)]
        struct Args {
            operation: String,
            #[serde(default)]
            content: String,
        }

        let args: Args =
            serde_json::from_str(arguments).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let session_id = ctx
            .metadata
            .get("session_id")
            .cloned()
            .unwrap_or_else(|| ctx.request_id.clone());

        match args.operation.as_str() {
            "read" => {
                let notes = SESSION_NOTES.lock().await;
                let entries = notes.get(&session_id).cloned().unwrap_or_default();
                let output = if entries.is_empty() {
                    "No session notes found.".to_string()
                } else {
                    entries
                        .iter()
                        .map(|e| format!("[{}] {}", e.timestamp, e.content))
                        .collect::<Vec<_>>()
                        .join("\n---\n")
                };
                Ok(ToolResult::success(output))
            }
            "write" => {
                if args.content.is_empty() {
                    return Err(ToolError::InvalidParams(
                        "Content required for 'write' operation".to_string(),
                    ));
                }
                let timestamp = chrono::Utc::now().to_rfc3339();
                let entry = SessionNoteEntry {
                    timestamp: timestamp.clone(),
                    content: args.content,
                };
                let mut notes = SESSION_NOTES.lock().await;
                notes
                    .entry(session_id.clone())
                    .or_insert_with(Vec::new)
                    .push(entry);
                Ok(ToolResult::success(format!("Note saved at {}", timestamp)))
            }
            _ => Err(ToolError::InvalidParams(
                "Operation must be 'read' or 'write'".to_string(),
            )),
        }
    }
}

// =============================================================================
// TOOL REGISTRY & VALIDATION (Extracted from zeroclaw-tool-call-parser)
// =============================================================================

/// Tool call parser that parses model outputs into tool calls.
pub struct ToolCallParser;

impl ToolCallParser {
    /// Parse a JSON string into tool calls.
    pub fn parse_tool_calls(json_str: &str) -> Result<Vec<yaatal_core::ToolCall>, ToolError> {
        #[derive(serde::Deserialize)]
        struct ToolCall {
            name: String,
            arguments: serde_json::Value,
        }

        #[derive(serde::Deserialize)]
        struct Root {
            tool_calls: Vec<ToolCall>,
        }

        let root: Root = serde_json::from_str(json_str)
            .map_err(|e| ToolError::InvalidParams(format!("JSON parse error: {}", e)))?;

        root.tool_calls
            .into_iter()
            .map(|tc| {
                Ok(yaatal_core::ToolCall {
                    name: tc.name,
                    arguments: serde_json::to_string(&tc.arguments)
                        .map_err(|e| ToolError::InvalidParams(e.to_string()))?,
                })
            })
            .collect()
    }

    /// Parse a function call from a markdown code block.
    pub fn parse_markdown_tool_calls(text: &str) -> Vec<yaatal_core::ToolCall> {
        let mut calls = Vec::new();

        // Look for markdown code blocks with JSON
        for block in text.split("```") {
            let trimmed = block.trim();
            if trimmed.starts_with("json") || trimmed.starts_with('{') {
                let content = if trimmed.starts_with("json") {
                    trimmed[4..].trim()
                } else {
                    trimmed
                };

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
                    if let Some(name) = parsed.get("name").and_then(|v| v.as_str()) {
                        if let Some(args) = parsed.get("arguments") {
                            calls.push(yaatal_core::ToolCall {
                                name: name.to_string(),
                                arguments: serde_json::to_string(args).unwrap_or_default(),
                            });
                        }
                    }
                }
            }
        }

        calls
    }
}
