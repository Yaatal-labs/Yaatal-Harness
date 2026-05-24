use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pipeline::traits::Identifiable;

/// Support configuration for ingestion-oriented workflows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestionConfig {
    pub keep_per_category: usize,
    pub max_enrichments_per_cycle: usize,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            keep_per_category: 50,
            max_enrichments_per_cycle: 25,
        }
    }
}

/// Context object for an ingestion run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestionQuery {
    pub run_id: String,
    pub timestamp: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub ai_budget_remaining: usize,
}

impl Default for IngestionQuery {
    fn default() -> Self {
        Self {
            run_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            last_run: None,
            ai_budget_remaining: 0,
        }
    }
}

impl IngestionQuery {
    pub fn new(ai_budget_remaining: usize) -> Self {
        Self {
            ai_budget_remaining,
            ..Default::default()
        }
    }
}

/// Raw content fetched before ranking or enrichment.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RawArticle {
    pub id: String,
    pub source_name: String,
    pub title: Option<String>,
    pub body: String,
    pub canonical_url: Option<String>,
    pub language: Option<String>,
    pub category: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub content_hash: String,
    pub ai_summary: Option<String>,
    pub enriched: bool,
    pub enriched_at: Option<DateTime<Utc>>,
    pub enrichment_error: Option<String>,
}

/// Per-source checkpoint state for ingestion-style jobs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceState {
    pub source_name: String,
    pub last_run: Option<DateTime<Utc>>,
    pub cursor: Option<String>,
    pub exhausted: bool,
}

impl Identifiable for RawArticle {
    fn id(&self) -> &str {
        &self.id
    }
}
