//! The audited exec wrapper from `docs/CLI-FIRST-TOOLS.md` ("Harness custody makes
//! CLI-first safe"): CLIs give ergonomics, Harness adds governance. [`AuditedExec`] runs
//! one external CLI command (a specific program + argument vector — not a free-form shell
//! string, so a caller cannot pivot to arbitrary shell the way `ShellTool` allows) with
//! the three custody pieces wrapped around it:
//!
//! 1. **Policy before exec** — `yaatal_policy::tool_policy::ToolPolicy` (CONTROL-LOOP
//!    slice 3: allowlist + per-run spend cap) is checked *before* the process spawns; a
//!    deny means the command never executes.
//! 2. **Timeout** — the child is killed once the deadline passes.
//! 3. **Audit always** — every invocation, allowed or denied, appends one `AuditEvent`
//!    (digests of the command line and its output, latency, and the policy verdict) to an
//!    `AuditStore`. A denied action is still an audited action; it just doesn't run.
//!
//! This lives in `yaatal-tools` rather than `yaatal-audit`/`yaatal-policy` because those
//! two crates are deliberately pure (one records, one decides — neither executes
//! anything), while this crate already owns process execution (`ShellTool`, `GitTool`)
//! and is where CLI-FIRST-TOOLS.md places the CLI-execution surface (`CliTool`).

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use uuid::Uuid;
use yaatal_audit::{ActionKind, AuditError, AuditEventBuilder, AuditStore, PolicyVerdict};
use yaatal_core::RequestContext;
use yaatal_policy::tool_policy::ToolPolicy;

/// What actually happened when the command ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    /// `None` if the process was killed (timeout) or terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// `true` when the child was killed because the deadline passed.
    pub timed_out: bool,
}

impl ExecOutput {
    /// `true` only for a clean, in-time, zero exit.
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

/// Why an invocation produced no [`ExecOutput`]. Deny-by-policy is a typed variant, per
/// CLI-FIRST-TOOLS.md's exit-code rule: "denied by policy vs. tool crashed should not
/// share an exit code" — callers can match on it rather than string-parse.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("denied by policy: {0}")]
    DeniedByPolicy(String),
    #[error("failed to spawn or drive '{program}': {source}")]
    Io {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Audit(#[from] AuditError),
}

/// Executes external CLI commands under policy + audit custody. Construct one per
/// runtime/actor and call [`AuditedExec::run`] per invocation.
pub struct AuditedExec {
    policy: Arc<dyn ToolPolicy>,
    store: Arc<dyn AuditStore>,
    actor: String,
    timeout: Duration,
}

impl AuditedExec {
    pub fn new(
        policy: Arc<dyn ToolPolicy>,
        store: Arc<dyn AuditStore>,
        actor: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            policy,
            store,
            actor: actor.into(),
            timeout,
        }
    }

    /// Run `program` with `args`. Policy is checked first (tool name = `program`); a
    /// deny returns [`ExecError::DeniedByPolicy`] *after* appending the deny
    /// `AuditEvent`, and the process is never spawned. An allowed run appends its
    /// `AuditEvent` (digests, latency, verdict) whether it exits cleanly, fails, or
    /// times out.
    pub async fn run(
        &self,
        ctx: &RequestContext,
        run_id: Uuid,
        program: &str,
        args: &[&str],
    ) -> Result<ExecOutput, ExecError> {
        let command_line = if args.is_empty() {
            program.to_string()
        } else {
            format!("{program} {}", args.join(" "))
        };

        let verdict = self.policy.check(ctx, run_id, program, &command_line).await;
        let builder = AuditEventBuilder::new(run_id, &self.actor, ActionKind::ToolCall, program)
            .policy_verdict(verdict.clone());

        if let PolicyVerdict::Deny(reason) = verdict {
            self.store
                .append(builder.finish(&command_line, &reason, false))
                .await?;
            return Err(ExecError::DeniedByPolicy(reason));
        }

        let started = Instant::now();
        let result = self.spawn_and_wait(program, args).await;
        let latency_ms = started.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                let audit_output = if output.timed_out {
                    format!("timed out after {}ms", self.timeout.as_millis())
                } else {
                    format!(
                        "exit={:?} stdout={} stderr={}",
                        output.exit_code, output.stdout, output.stderr
                    )
                };
                let mut event = builder.finish(&command_line, &audit_output, output.success());
                event.latency_ms = latency_ms;
                self.store.append(event).await?;
                Ok(output)
            }
            Err(source) => {
                let mut event = builder.finish(&command_line, &source.to_string(), false);
                event.latency_ms = latency_ms;
                self.store.append(event).await?;
                Err(ExecError::Io {
                    program: program.to_string(),
                    source,
                })
            }
        }
    }

    /// Spawn via `std::process::Command` and poll `try_wait` until exit or deadline,
    /// killing the child on expiry. The poll sleep is tokio's only involvement (this
    /// method is already async for the caller's sake); the process handling itself is
    /// std-only.
    ///
    /// ponytail: output is read after the child exits, so a child that fills the OS pipe
    /// buffer (~64KiB) before the deadline will stall until killed rather than stream.
    /// Fine for the small, sharp CLIs this wrapper is for; upgrade path is reader
    /// threads (or `tokio::process`) if a governed CLI ever legitimately emits megabytes.
    async fn spawn_and_wait(&self, program: &str, args: &[&str]) -> std::io::Result<ExecOutput> {
        use std::io::Read;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let deadline = Instant::now() + self.timeout;
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait()? {
                break (Some(status), false);
            }
            if Instant::now() >= deadline {
                child.kill()?;
                child.wait()?; // reap; also closes our read end cleanly
                break (None, true);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };

        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }

        Ok(ExecOutput {
            exit_code: status.and_then(|s| s.code()),
            stdout,
            stderr,
            timed_out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaatal_audit::MemoryAuditStore;
    use yaatal_policy::tool_policy::ToolPolicyGate;

    fn gate_allowing(tools: &[&str], store: &Arc<MemoryAuditStore>) -> Arc<dyn ToolPolicy> {
        Arc::new(ToolPolicyGate::new(
            tools.iter().map(|t| t.to_string()),
            None,
            Arc::clone(store) as Arc<dyn AuditStore>,
        ))
    }

    #[tokio::test]
    async fn allowed_command_runs_and_is_audited_with_verdict() {
        let store = Arc::new(MemoryAuditStore::new());
        let exec = AuditedExec::new(
            gate_allowing(&["echo"], &store),
            Arc::clone(&store) as Arc<dyn AuditStore>,
            "engine:test",
            Duration::from_secs(5),
        );

        let ctx = RequestContext::new("test");
        let run_id = Uuid::new_v4();
        let output = exec
            .run(&ctx, run_id, "echo", &["hello", "harness"])
            .await
            .expect("echo runs");

        assert!(output.success());
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.trim(), "hello harness");

        let events = store.by_run(run_id).await.expect("by_run");
        assert_eq!(events.len(), 1);
        assert!(events[0].success);
        assert_eq!(events[0].action_name, "echo");
        assert_eq!(events[0].policy_verdicts, vec![PolicyVerdict::Allow]);
    }

    #[tokio::test]
    async fn denied_command_never_executes_and_deny_is_audited() {
        let store = Arc::new(MemoryAuditStore::new());
        let exec = AuditedExec::new(
            gate_allowing(&["echo"], &store), // "touch" is NOT allowlisted
            Arc::clone(&store) as Arc<dyn AuditStore>,
            "engine:test",
            Duration::from_secs(5),
        );

        // If the deny leaked past the gate, this file would exist afterwards.
        let marker = std::env::temp_dir().join(format!("yaatal-exec-deny-{}", Uuid::new_v4()));
        let marker_str = marker.to_string_lossy().to_string();

        let ctx = RequestContext::new("test");
        let run_id = Uuid::new_v4();
        let result = exec.run(&ctx, run_id, "touch", &[&marker_str]).await;

        assert!(matches!(result, Err(ExecError::DeniedByPolicy(_))));
        assert!(!marker.exists(), "denied command must not have executed");

        let events = store.by_run(run_id).await.expect("by_run");
        assert_eq!(
            events.len(),
            1,
            "a denied action is still an audited action"
        );
        assert!(!events[0].success);
        assert!(events[0].policy_verdicts[0].is_deny());
    }

    #[tokio::test]
    async fn timed_out_command_is_killed_and_audited_as_failure() {
        let store = Arc::new(MemoryAuditStore::new());
        let exec = AuditedExec::new(
            gate_allowing(&["sleep"], &store),
            Arc::clone(&store) as Arc<dyn AuditStore>,
            "engine:test",
            Duration::from_millis(100),
        );

        let ctx = RequestContext::new("test");
        let run_id = Uuid::new_v4();
        let started = Instant::now();
        let output = exec
            .run(&ctx, run_id, "sleep", &["30"])
            .await
            .expect("spawn succeeds even though the run times out");

        assert!(output.timed_out);
        assert!(!output.success());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "child must have been killed at the deadline, not waited for"
        );

        let events = store.by_run(run_id).await.expect("by_run");
        assert_eq!(events.len(), 1);
        assert!(!events[0].success);
    }

    #[tokio::test]
    async fn spend_cap_denies_the_next_invocation() {
        let store = Arc::new(MemoryAuditStore::new());
        let run_id = Uuid::new_v4();
        // Seed the run right up to its cap.
        store
            .append(
                AuditEventBuilder::new(run_id, "engine:test", ActionKind::ToolCall, "echo")
                    .cost(1.0)
                    .finish("in", "out", true),
            )
            .await
            .expect("seed");

        let policy: Arc<dyn ToolPolicy> = Arc::new(ToolPolicyGate::new(
            ["echo".to_string()],
            Some(1.0),
            Arc::clone(&store) as Arc<dyn AuditStore>,
        ));
        let exec = AuditedExec::new(
            policy,
            Arc::clone(&store) as Arc<dyn AuditStore>,
            "engine:test",
            Duration::from_secs(5),
        );

        let ctx = RequestContext::new("test");
        let result = exec.run(&ctx, run_id, "echo", &["over budget"]).await;
        assert!(matches!(result, Err(ExecError::DeniedByPolicy(_))));

        let events = store.by_run(run_id).await.expect("by_run");
        assert_eq!(events.len(), 2, "seed event + audited deny");
        assert!(events[1].policy_verdicts[0].is_deny());
    }
}
