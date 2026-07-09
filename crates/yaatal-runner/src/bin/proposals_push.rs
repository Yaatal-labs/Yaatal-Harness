//! `yaatal-proposals-push` — push local L1 proposals to the Engine review queue.
//!
//! Usage: `yaatal-proposals-push <proposals.jsonl>`
//!
//! Required environment:
//! - `YAATAL_ENGINE_URL` — base Engine URL
//! - `YAATAL_TOKEN` — JWT for the ops service account

use std::process::ExitCode;

use yaatal_runner::proposals_push::push_file;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{}", serde_json::json!({ "error": message }));
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: yaatal-proposals-push <proposals.jsonl>".to_string())?;
    let engine_url = std::env::var("YAATAL_ENGINE_URL")
        .map_err(|_| "YAATAL_ENGINE_URL is required".to_string())?;
    let token =
        std::env::var("YAATAL_TOKEN").map_err(|_| "YAATAL_TOKEN is required".to_string())?;

    let summary = push_file(path, &engine_url, &token)
        .await
        .map_err(|e| e.to_string())?;
    let rendered = serde_json::to_string(&summary)
        .map_err(|e| format!("failed to serialize push summary: {e}"))?;
    println!("{rendered}");
    Ok(())
}
