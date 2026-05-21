//! Tool system for the Yaatal AI harness.
//!
//! This crate provides the tool execution infrastructure extracted from
//! zeroclaw-labs/zeroclaw. It includes:
//!
//! - [`ToolExecutor`]: Runtime for managing and executing tools
//! - Safe built-in tools by default: File Read and Session Notes
//! - R&D built-in tools behind explicit Cargo features: Shell, File Write, Git,
//!   Web Fetch, and Search
//! - Tool registry with validation
//! - Max steps limiting (prevents infinite loops)
//! - File-backed session persistence for long-running agents
//! - [`intent_router`]: Intent-based tool routing (Picovoice pattern)
//!
//! Dangerous tools are not available in the default build. Enable
//! `r-and-d-tools` or specific features such as `local-shell`, `file-write`,
//! `git`, `web-fetch`, and `web-search` only in trusted R&D contexts.
//!
//! ## Example
//!
//! ```rust
//! use yaatal_tools::{ToolExecutor, BuiltinTool};
//! use yaatal_core::RequestContext;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), yaatal_core::ToolError> {
//!     let executor = ToolExecutor::new();
//!     executor.register_builtin(BuiltinTool::SessionNote).await?;
//!
//!     let ctx = RequestContext::new("test");
//!     let result = executor
//!         .execute(&ctx, "session_note", r#"{"operation": "read"}"#)
//!         .await?;
//!     println!("{:?}", result);
//!     Ok(())
//! }
//! ```

pub mod intent_router;

use std::collections::HashMap;
#[cfg(any(feature = "file-read", feature = "file-write"))]
use std::path::Path;
#[cfg(any(
    feature = "file-read",
    feature = "file-write",
    feature = "session-note"
))]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(any(
    feature = "local-shell",
    feature = "file-read",
    feature = "file-write",
    feature = "git",
    feature = "web-fetch",
    feature = "web-search",
    feature = "session-note"
))]
use std::sync::LazyLock;
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
    pub async fn register_builtin(&self, tool: BuiltinTool) -> Result<(), ToolError> {
        let boxed: Arc<dyn Tool> = match tool {
            BuiltinTool::Shell => {
                #[cfg(feature = "local-shell")]
                {
                    Ok(Arc::new(ShellTool::new()) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "local-shell"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::FileRead => {
                #[cfg(feature = "file-read")]
                {
                    let workspace_dir = self.workspace_dir().await;
                    Ok(Arc::new(FileReadTool::new().with_base_dir(workspace_dir)) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "file-read"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::FileWrite => {
                #[cfg(feature = "file-write")]
                {
                    let workspace_dir = self.workspace_dir().await;
                    Ok(Arc::new(FileWriteTool::new().with_base_dir(workspace_dir))
                        as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "file-write"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::Git => {
                #[cfg(feature = "git")]
                {
                    Ok(Arc::new(GitTool::new()) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "git"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::WebFetch => {
                #[cfg(feature = "web-fetch")]
                {
                    Ok(Arc::new(WebFetchTool::new()) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "web-fetch"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::Search => {
                #[cfg(feature = "web-search")]
                {
                    Ok(Arc::new(SearchTool::new()) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "web-search"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
            BuiltinTool::SessionNote => {
                #[cfg(feature = "session-note")]
                {
                    let workspace_dir = self.workspace_dir().await;
                    Ok(Arc::new(SessionNoteTool::for_workspace(workspace_dir)) as Arc<dyn Tool>)
                }

                #[cfg(not(feature = "session-note"))]
                {
                    Err(disabled_builtin_error(tool))
                }
            }
        }?;
        self.register(boxed).await;
        Ok(())
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl BuiltinTool {
    /// Runtime tool name registered for this built-in.
    pub fn tool_name(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::Git => "git",
            Self::WebFetch => "web_fetch",
            Self::Search => "search",
            Self::SessionNote => "session_note",
        }
    }

    /// Cargo feature required to register this built-in.
    pub fn required_feature(self) -> &'static str {
        match self {
            Self::Shell => "local-shell",
            Self::FileRead => "file-read",
            Self::FileWrite => "file-write",
            Self::Git => "git",
            Self::WebFetch => "web-fetch",
            Self::Search => "web-search",
            Self::SessionNote => "session-note",
        }
    }
}

#[allow(dead_code)]
fn disabled_builtin_error(tool: BuiltinTool) -> ToolError {
    ToolError::PermissionDenied(format!(
        "Built-in tool '{}' is disabled; enable Cargo feature '{}'",
        tool.tool_name(),
        tool.required_feature()
    ))
}

#[cfg(any(feature = "file-read", feature = "file-write"))]
fn canonicalize_path(path: &Path) -> Result<PathBuf, ToolError> {
    std::fs::canonicalize(path).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
}

#[cfg(any(feature = "file-read", feature = "file-write"))]
fn workspace_root(base_dir: &Option<String>) -> Result<PathBuf, ToolError> {
    let base = if let Some(base_dir) = base_dir {
        PathBuf::from(base_dir)
    } else {
        std::env::current_dir().map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
    };

    canonicalize_path(&base)
}

#[cfg(any(feature = "file-read", feature = "file-write"))]
fn ensure_within_workspace(workspace: &Path, candidate: &Path) -> Result<(), ToolError> {
    if candidate.starts_with(workspace) {
        Ok(())
    } else {
        Err(ToolError::PermissionDenied(format!(
            "Path '{}' is outside workspace '{}'",
            candidate.display(),
            workspace.display()
        )))
    }
}

#[cfg(any(feature = "file-read", feature = "file-write"))]
fn resolve_scoped_path(
    base_dir: &Option<String>,
    path: &str,
    allow_missing_leaf: bool,
) -> Result<PathBuf, ToolError> {
    let workspace = workspace_root(base_dir)?;
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        workspace.join(path)
    };

    if allow_missing_leaf {
        let parent = candidate.parent().ok_or_else(|| {
            ToolError::InvalidParams(format!("Invalid writable path '{}'", candidate.display()))
        })?;
        let canonical_parent = canonicalize_path(parent)?;
        ensure_within_workspace(&workspace, &canonical_parent)?;

        let file_name = candidate.file_name().ok_or_else(|| {
            ToolError::InvalidParams(format!("Invalid writable path '{}'", candidate.display()))
        })?;

        Ok(canonical_parent.join(file_name))
    } else {
        let canonical_candidate = canonicalize_path(&candidate)?;
        ensure_within_workspace(&workspace, &canonical_candidate)?;
        Ok(canonical_candidate)
    }
}

/// Shell command execution tool.
#[cfg(feature = "local-shell")]
pub struct ShellTool;

#[cfg(feature = "local-shell")]
impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "local-shell")]
impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "local-shell")]
#[async_trait::async_trait]
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
#[cfg(feature = "file-read")]
pub struct FileReadTool {
    /// Base directory for resolving relative paths ( ACI principle: always absolute paths)
    base_dir: Option<String>,
}

#[cfg(feature = "file-read")]
impl FileReadTool {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Set base directory for resolving relative paths.
    pub fn with_base_dir(mut self, dir: impl Into<String>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolError> {
        resolve_scoped_path(&self.base_dir, path, false)
    }
}

#[cfg(feature = "file-read")]
impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "file-read")]
#[async_trait::async_trait]
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
        let resolved_path = self.resolve_path(&args.path)?;

        tracing::info!(file_path = %resolved_path.display(), request_id = %ctx.request_id);

        let content = tokio::fs::read_to_string(&resolved_path)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let lines: Vec<&str> = content.lines().take(args.limit).collect();
        let result = lines.join("\n");

        Ok(ToolResult::success(result))
    }
}

/// File write tool.
#[cfg(feature = "file-write")]
pub struct FileWriteTool {
    /// Base directory for resolving relative paths
    base_dir: Option<String>,
}

#[cfg(feature = "file-write")]
impl FileWriteTool {
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Set base directory for resolving relative paths.
    pub fn with_base_dir(mut self, dir: impl Into<String>) -> Self {
        self.base_dir = Some(dir.into());
        self
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolError> {
        resolve_scoped_path(&self.base_dir, path, true)
    }
}

#[cfg(feature = "file-write")]
impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "file-write")]
#[async_trait::async_trait]
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
        let resolved_path = self.resolve_path(&args.path)?;

        tracing::info!(file_path = %resolved_path.display(), request_id = %ctx.request_id);

        tokio::fs::write(&resolved_path, &args.content)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        Ok(ToolResult::success(format!(
            "Written {} bytes to {}",
            args.content.len(),
            resolved_path.display()
        )))
    }
}

/// Git operations tool.
#[cfg(feature = "git")]
pub struct GitTool;

#[cfg(feature = "git")]
impl GitTool {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "git")]
impl Default for GitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "git")]
#[async_trait::async_trait]
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
#[cfg(feature = "web-fetch")]
pub struct WebFetchTool;

#[cfg(feature = "web-fetch")]
impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "web-fetch")]
impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "web-fetch")]
#[async_trait::async_trait]
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
#[cfg(feature = "web-search")]
pub struct SearchTool;

#[cfg(feature = "web-search")]
impl SearchTool {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "web-search")]
impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "web-search")]
#[async_trait::async_trait]
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

#[cfg(feature = "session-note")]
use tokio::sync::Mutex;

#[cfg(feature = "session-note")]
const DEFAULT_SESSION_NOTE_PATH: &str = ".yaatal/session_notes.json";

/// One persisted session-note entry.
#[cfg(feature = "session-note")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionNoteEntry {
    pub timestamp: String,
    pub content: String,
}

/// Trait for backing session notes with durable or custom storage.
#[cfg(feature = "session-note")]
#[async_trait::async_trait]
pub trait SessionNoteStore: Send + Sync {
    async fn read(&self, session_id: &str) -> Result<Vec<SessionNoteEntry>, ToolError>;
    async fn append(&self, session_id: &str, entry: SessionNoteEntry) -> Result<(), ToolError>;
}

#[cfg(feature = "session-note")]
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct SessionNoteData {
    #[serde(default)]
    sessions: HashMap<String, Vec<SessionNoteEntry>>,
}

/// JSON file-backed session note store.
#[cfg(feature = "session-note")]
pub struct FileSessionNoteStore {
    path: PathBuf,
    lock: Mutex<()>,
}

#[cfg(feature = "session-note")]
impl FileSessionNoteStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    async fn read_data(&self) -> Result<SessionNoteData, ToolError> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(content) => {
                if content.trim().is_empty() {
                    Ok(SessionNoteData::default())
                } else {
                    serde_json::from_str(&content).map_err(|e| {
                        ToolError::ExecutionFailed(format!(
                            "Failed to parse session note store '{}': {}",
                            self.path.display(),
                            e
                        ))
                    })
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SessionNoteData::default()),
            Err(e) => Err(ToolError::ExecutionFailed(format!(
                "Failed to read session note store '{}': {}",
                self.path.display(),
                e
            ))),
        }
    }

    async fn write_data(&self, data: &SessionNoteData) -> Result<(), ToolError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    ToolError::ExecutionFailed(format!(
                        "Failed to create session note directory '{}': {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        let content = serde_json::to_string_pretty(data).map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to serialize session notes: {}", e))
        })?;
        let temp_path = self.path.with_extension("json.tmp");

        tokio::fs::write(&temp_path, content).await.map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to write temporary session note store '{}': {}",
                temp_path.display(),
                e
            ))
        })?;

        match tokio::fs::rename(&temp_path, &self.path).await {
            Ok(()) => Ok(()),
            Err(rename_error) => {
                match tokio::fs::remove_file(&self.path).await {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return Err(ToolError::ExecutionFailed(format!(
                            "Failed to replace session note store '{}': {}; rename failed with {}",
                            self.path.display(),
                            e,
                            rename_error
                        )));
                    }
                }

                tokio::fs::rename(&temp_path, &self.path)
                    .await
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&temp_path);
                        ToolError::ExecutionFailed(format!(
                            "Failed to move temporary session note store '{}' to '{}': {}",
                            temp_path.display(),
                            self.path.display(),
                            e
                        ))
                    })
            }
        }
    }
}

#[cfg(feature = "session-note")]
#[async_trait::async_trait]
impl SessionNoteStore for FileSessionNoteStore {
    async fn read(&self, session_id: &str) -> Result<Vec<SessionNoteEntry>, ToolError> {
        let _guard = self.lock.lock().await;
        let data = self.read_data().await?;
        Ok(data.sessions.get(session_id).cloned().unwrap_or_default())
    }

    async fn append(&self, session_id: &str, entry: SessionNoteEntry) -> Result<(), ToolError> {
        let _guard = self.lock.lock().await;
        let mut data = self.read_data().await?;
        data.sessions
            .entry(session_id.to_string())
            .or_insert_with(Vec::new)
            .push(entry);
        self.write_data(&data).await
    }
}

/// Session note tool for reading/writing persistent notes.
/// This is critical for long-running agents that need to resume after crashes.
#[cfg(feature = "session-note")]
pub struct SessionNoteTool {
    store: Arc<dyn SessionNoteStore>,
}

#[cfg(feature = "session-note")]
impl SessionNoteTool {
    pub fn new() -> Self {
        let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::for_workspace(workspace)
    }

    pub fn for_workspace(workspace: impl Into<PathBuf>) -> Self {
        let path = workspace.into().join(DEFAULT_SESSION_NOTE_PATH);
        Self::with_store(Arc::new(FileSessionNoteStore::new(path)))
    }

    pub fn with_store(store: Arc<dyn SessionNoteStore>) -> Self {
        Self { store }
    }
}

#[cfg(feature = "session-note")]
impl Default for SessionNoteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "session-note")]
#[async_trait::async_trait]
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
                let entries = self.store.read(&session_id).await?;
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
                self.store.append(&session_id, entry).await?;
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
            #[serde(default)]
            id: Option<String>,
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
                    id: tc.id,
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
                                id: parsed
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "file-read", feature = "session-note"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(feature = "file-read")]
    use yaatal_core::Tool;

    async fn registered_tool_names(executor: &ToolExecutor) -> Vec<String> {
        let mut names = executor
            .list_tools()
            .await
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[cfg(all(feature = "file-read", feature = "session-note"))]
    #[tokio::test]
    async fn safe_builtins_register_with_default_features() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::FileRead).await?;
        executor.register_builtin(BuiltinTool::SessionNote).await?;

        let names = registered_tool_names(&executor).await;

        assert_eq!(names, vec!["file_read", "session_note"]);
        Ok(())
    }

    #[cfg(feature = "session-note")]
    #[tokio::test]
    async fn session_note_persists_across_executor_reopen() -> Result<(), ToolError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("yaatal-session-notes-{unique}"));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let workspace_dir = workspace.to_string_lossy().to_string();
        let mut ctx = RequestContext::new("resume-test-request");
        ctx.metadata
            .insert("session_id".to_string(), "session-a".to_string());

        let executor = ToolExecutor::new().with_workspace_dir(workspace_dir.clone());
        executor.register_builtin(BuiltinTool::SessionNote).await?;
        let write_args = serde_json::json!({
            "operation": "write",
            "content": "persist me across tool instances"
        });
        executor
            .execute(&ctx, "session_note", &write_args.to_string())
            .await?;

        let reopened = ToolExecutor::new().with_workspace_dir(workspace_dir);
        reopened.register_builtin(BuiltinTool::SessionNote).await?;
        let read_args = serde_json::json!({
            "operation": "read"
        });
        let result = reopened
            .execute(&ctx, "session_note", &read_args.to_string())
            .await?;

        assert!(result.success);
        assert!(result.content.contains("persist me across tool instances"));
        assert!(workspace.join(DEFAULT_SESSION_NOTE_PATH).exists());

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[cfg(feature = "session-note")]
    #[tokio::test]
    async fn file_session_note_store_reopens_written_notes() -> Result<(), ToolError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("yaatal-session-store-{unique}"));
        let path = root.join("notes.json");

        let store = FileSessionNoteStore::new(path.clone());
        store
            .append(
                "session-a",
                SessionNoteEntry {
                    timestamp: "2026-05-21T00:00:00Z".to_string(),
                    content: "reopen direct store".to_string(),
                },
            )
            .await?;

        let reopened = FileSessionNoteStore::new(path);
        let entries = reopened.read("session-a").await?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "reopen direct store");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[cfg(feature = "session-note")]
    #[tokio::test]
    async fn file_session_note_store_isolates_session_ids() -> Result<(), ToolError> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("yaatal-session-isolation-{unique}"));
        let store = FileSessionNoteStore::new(root.join("notes.json"));

        store
            .append(
                "session-a",
                SessionNoteEntry {
                    timestamp: "2026-05-21T00:00:00Z".to_string(),
                    content: "only session a".to_string(),
                },
            )
            .await?;
        store
            .append(
                "session-b",
                SessionNoteEntry {
                    timestamp: "2026-05-21T00:00:01Z".to_string(),
                    content: "only session b".to_string(),
                },
            )
            .await?;

        let session_a = store.read("session-a").await?;
        let session_b = store.read("session-b").await?;

        assert_eq!(session_a.len(), 1);
        assert_eq!(session_a[0].content, "only session a");
        assert_eq!(session_b.len(), 1);
        assert_eq!(session_b[0].content, "only session b");

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[cfg(not(any(
        feature = "local-shell",
        feature = "file-write",
        feature = "git",
        feature = "web-fetch",
        feature = "web-search"
    )))]
    #[tokio::test]
    async fn unsafe_builtins_are_rejected_without_rd_features() {
        let executor = ToolExecutor::new();
        let disabled_tools = [
            BuiltinTool::Shell,
            BuiltinTool::FileWrite,
            BuiltinTool::Git,
            BuiltinTool::WebFetch,
            BuiltinTool::Search,
        ];

        for tool in disabled_tools {
            let result = executor.register_builtin(tool).await;
            match result {
                Err(ToolError::PermissionDenied(message)) => {
                    assert!(message.contains(tool.tool_name()));
                    assert!(message.contains(tool.required_feature()));
                }
                other => panic!("expected permission denial for {tool:?}, got {other:?}"),
            }
        }

        assert!(registered_tool_names(&executor).await.is_empty());
    }

    #[cfg(feature = "local-shell")]
    #[tokio::test]
    async fn shell_builtin_registers_when_enabled() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::Shell).await?;

        assert_eq!(registered_tool_names(&executor).await, vec!["shell"]);
        Ok(())
    }

    #[cfg(feature = "file-write")]
    #[tokio::test]
    async fn file_write_builtin_registers_when_enabled() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::FileWrite).await?;

        assert_eq!(registered_tool_names(&executor).await, vec!["file_write"]);
        Ok(())
    }

    #[cfg(feature = "git")]
    #[tokio::test]
    async fn git_builtin_registers_when_enabled() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::Git).await?;

        assert_eq!(registered_tool_names(&executor).await, vec!["git"]);
        Ok(())
    }

    #[cfg(feature = "web-fetch")]
    #[tokio::test]
    async fn web_fetch_builtin_registers_when_enabled() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::WebFetch).await?;

        assert_eq!(registered_tool_names(&executor).await, vec!["web_fetch"]);
        Ok(())
    }

    #[cfg(feature = "web-search")]
    #[tokio::test]
    async fn web_search_builtin_registers_when_enabled() -> Result<(), ToolError> {
        let executor = ToolExecutor::new();

        executor.register_builtin(BuiltinTool::Search).await?;

        assert_eq!(registered_tool_names(&executor).await, vec!["search"]);
        Ok(())
    }

    #[cfg(feature = "file-read")]
    #[tokio::test]
    async fn file_read_rejects_paths_outside_workspace() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("yaatal-tools-{unique}"));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let outside = root.join("outside.txt");
        std::fs::write(&outside, "secret").unwrap();

        let tool = FileReadTool::new().with_base_dir(workspace.to_string_lossy().to_string());
        let ctx = RequestContext::new("test");
        let arguments = serde_json::json!({
            "path": outside.to_string_lossy().to_string()
        });

        let result = tool.execute(&ctx, &arguments.to_string()).await;

        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));

        let _ = std::fs::remove_dir_all(&root);
    }
}
