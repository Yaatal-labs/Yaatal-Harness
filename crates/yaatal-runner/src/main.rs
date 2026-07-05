//! `yaatal-ops-runner` — the Harness's first L0-operating tenant.
//!
//! Agent-first contract, identical in spirit to the SDK's `yaatal` CLI:
//! one JSON value to stdout on success (exit 0 if the run's eval passed, 1 if
//! it failed), and `{"error": ...}` to stderr on any usage/execution error
//! (exit 2). No interactive prompts. Config for the executed CLI flows through
//! the process environment (`YAATAL_ENGINE_URL`, `YAATAL_TOKEN`) untouched.
//!
//! Usage: `yaatal-ops-runner <runbook.json>`

use std::process::ExitCode;

use yaatal_runner::{execute, Runbook};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(passed) => {
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(message) => {
            // One-line JSON error to stderr, exit 2 — the usage/error channel.
            eprintln!("{}", serde_json::json!({ "error": message }));
            ExitCode::from(2)
        }
    }
}

/// Returns Ok(eval_passed) on a completed run, Err(message) on a usage/IO error.
async fn run() -> Result<bool, String> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: yaatal-ops-runner <runbook.json>".to_string())?;

    let json =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read runbook '{path}': {e}"))?;
    let runbook = Runbook::from_json(&json).map_err(|e| e.to_string())?;

    let summary = execute(&runbook).await.map_err(|e| e.to_string())?;

    // The single JSON value on stdout.
    let rendered =
        serde_json::to_string(&summary).map_err(|e| format!("failed to serialize summary: {e}"))?;
    println!("{rendered}");

    Ok(summary.eval.passed)
}
