//! Testable MVP pipeline example.
//!
//! This example demonstrates the full Yaatal search pipeline with:
//! - Tool-augmented enrichment (fetch metadata for URLs)
//! - LLM-based reranking (score candidates with LLM)
//! - Policy filtering (apply rules to results)
//!
//! ## Running
//!
//! ```bash
//! cargo run --example mvp_pipeline
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use yaatal_core::{
    Candidate, HarnessError, PolicyEngine, PolicyResult, RequestContext, Retriever, ScoredCandidate,
};
use yaatal_memory::InMemoryStore;
use yaatal_models::MockProvider;
use yaatal_search::enricher::{EnrichConfig, EnrichmentExecutor, IntoCandidates};
use yaatal_search::reranker::LlmReranker;
use yaatal_tools::ToolExecutor;

// =============================================================================
// MOCK IMPLEMENTATIONS (Replace with real implementations)
// =============================================================================

/// Mock retriever that returns test candidates.
struct MockRetriever;

#[async_trait::async_trait]
impl Retriever for MockRetriever {
    async fn retrieve(
        &self,
        _ctx: &RequestContext,
        query: &str,
    ) -> Result<Vec<Candidate>, HarnessError> {
        println!("[Retriever] Query: {}", query);

        // Return mock candidates based on query
        let candidates = vec![
            Candidate {
                id: "doc1".to_string(),
                attributes: HashMap::from([
                    ("title".to_string(), "Rust Programming Guide".to_string()),
                    ("url".to_string(), "https://example.com/rust-guide".to_string()),
                    ("content".to_string(), "A comprehensive guide to Rust programming language including ownership, borrowing, and async.".to_string()),
                ]),
            },
            Candidate {
                id: "doc2".to_string(),
                attributes: HashMap::from([
                    ("title".to_string(), "Python vs Rust".to_string()),
                    ("url".to_string(), "https://example.com/python-rust".to_string()),
                    ("content".to_string(), "Comparing Python and Rust: performance, readability, and use cases.".to_string()),
                ]),
            },
            Candidate {
                id: "doc3".to_string(),
                attributes: HashMap::from([
                    ("title".to_string(), "Web Development 2024".to_string()),
                    ("url".to_string(), "https://example.com/web-dev".to_string()),
                    ("content".to_string(), "Modern web development trends including React, Rust backends, and edge computing.".to_string()),
                ]),
            },
            Candidate {
                id: "doc4".to_string(),
                attributes: HashMap::from([
                    ("title".to_string(), "AI/ML Trends".to_string()),
                    ("url".to_string(), "https://example.com/ai-trends".to_string()),
                    ("content".to_string(), "Machine learning and AI developments in 2024.".to_string()),
                ]),
            },
        ];

        println!("[Retriever] Found {} candidates", candidates.len());
        Ok(candidates)
    }
}

/// Mock policy that allows all candidates above a score threshold.
struct ThresholdPolicy {
    threshold: f32,
}

impl ThresholdPolicy {
    fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

#[async_trait::async_trait]
impl PolicyEngine for ThresholdPolicy {
    async fn evaluate(
        &self,
        _ctx: &RequestContext,
        items: Vec<ScoredCandidate>,
    ) -> Result<PolicyResult, HarnessError> {
        println!(
            "[Policy] Evaluating {} items (threshold: {})",
            items.len(),
            self.threshold
        );

        let (allowed, denied): (Vec<_>, Vec<_>) =
            items.into_iter().partition(|sc| sc.score >= self.threshold);

        let denied_with_reason: Vec<(ScoredCandidate, String)> = denied
            .into_iter()
            .map(|sc| {
                let reason = format!("Score {} below threshold {}", sc.score, self.threshold);
                (sc, reason)
            })
            .collect();

        println!(
            "[Policy] Allowed: {}, Denied: {}",
            allowed.len(),
            denied_with_reason.len()
        );
        Ok(PolicyResult {
            allowed,
            denied: denied_with_reason,
        })
    }
}

// =============================================================================
// PIPELINE ORCHESTRATOR
// =============================================================================

/// Full search pipeline orchestrator.
struct SearchPipeline {
    retriever: Arc<dyn Retriever>,
    enricher: Option<Arc<EnrichmentExecutor>>,
    reranker: Option<Arc<LlmReranker>>,
    policy: Arc<dyn PolicyEngine>,
}

impl SearchPipeline {
    fn new(retriever: Arc<dyn Retriever>, policy: Arc<dyn PolicyEngine>) -> Self {
        Self {
            retriever,
            enricher: None,
            reranker: None,
            policy,
        }
    }

    fn with_enrichment(mut self, enricher: Arc<EnrichmentExecutor>) -> Self {
        self.enricher = Some(enricher);
        self
    }

    fn with_reranker(mut self, reranker: Arc<LlmReranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Execute the full pipeline.
    async fn search(
        &self,
        query: &str,
        ctx: &RequestContext,
    ) -> Result<PolicyResult, HarnessError> {
        println!("\n========================================");
        println!("Pipeline Search: {}", query);
        println!("========================================\n");

        // Stage 1: Retrieve candidates
        println!("[Stage 1] Retrieval");
        let candidates = self.retriever.retrieve(ctx, query).await?;
        println!("  -> Retrieved {} candidates\n", candidates.len());

        // Stage 2: Enrichment (optional)
        let candidates = match &self.enricher {
            Some(e) => {
                println!("[Stage 2] Enrichment");
                let results = e.enrich(candidates, ctx).await?;
                println!("  -> Enriched {} candidates\n", results.len());
                results.into_candidates()
            }
            None => {
                println!("[Stage 2] Enrichment - SKIPPED (not configured)\n");
                candidates
            }
        };

        // Stage 3: Rerank with LLM (optional)
        let scored = match &self.reranker {
            Some(r) => {
                println!("[Stage 3] LLM Reranking");
                let ranked = r.rerank(query, candidates, ctx).await?;
                println!("  -> Reranked {} candidates\n", ranked.len());
                ranked
            }
            None => {
                println!("[Stage 3] LLM Reranking - SKIPPED (not configured)\n");
                candidates
                    .into_iter()
                    .map(|c| ScoredCandidate {
                        candidate: c,
                        score: 5.0,
                        metadata: None,
                    })
                    .collect()
            }
        };

        // Stage 4: Policy evaluation
        println!("[Stage 4] Policy Evaluation");
        let result = self.policy.evaluate(ctx, scored).await?;
        println!(
            "  -> Allowed: {}, Denied: {}\n",
            result.allowed.len(),
            result.denied.len()
        );

        Ok(result)
    }
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n===========================================");
    println!("  Yaatal MVP Pipeline Example");
    println!("===========================================\n");

    // Setup components
    let llm = Arc::new(MockProvider::new(
        r#"[{"id": "doc1", "score": 9.0, "reason": "directly covers Rust programming"},
            {"id": "doc2", "score": 7.5, "reason": "mentions Rust alongside Python"},
            {"id": "doc3", "score": 6.0, "reason": "Rust backend mentioned"},
            {"id": "doc4", "score": 4.0, "reason": "unrelated to Rust"}]"#
            .to_string(),
    ));
    let tool_executor = Arc::new(ToolExecutor::new());
    let memory_store = Arc::new(InMemoryStore::new());

    // Create enrichment executor
    let enrich_config = EnrichConfig {
        fetch_metadata: true,
        fetch_summaries: false,
        max_candidates_to_enrich: 5,
        use_cache: true,
    };
    let enricher = Arc::new(
        EnrichmentExecutor::new(tool_executor)
            .with_memory(memory_store.clone())
            .with_config(enrich_config),
    );

    // Create LLM reranker
    let reranker = Arc::new(LlmReranker::new(llm));

    // Create pipeline
    let pipeline =
        SearchPipeline::new(Arc::new(MockRetriever), Arc::new(ThresholdPolicy::new(5.0)))
            .with_enrichment(enricher)
            .with_reranker(reranker);

    // Run search
    let ctx = RequestContext::new("test-request-123");
    let result = pipeline.search("Rust programming", &ctx).await?;

    // Print results
    println!("===========================================");
    println!("  RESULTS");
    println!("===========================================\n");

    println!("Allowed ({} items):", result.allowed.len());
    for (i, sc) in result.allowed.iter().enumerate() {
        let title = sc
            .candidate
            .attributes
            .get("title")
            .map(String::as_str)
            .unwrap_or("<no title>");
        let reason = sc
            .metadata
            .as_ref()
            .and_then(|m| m.get("reason"))
            .map(|s| s.as_str())
            .unwrap_or("N/A");
        println!("  {}. {} (score: {:.1})", i + 1, title, sc.score);
        println!("     Reason: {}", reason);
    }

    if !result.denied.is_empty() {
        println!("\nDenied ({} items):", result.denied.len());
        for (sc, reason) in &result.denied {
            let title = sc
                .candidate
                .attributes
                .get("title")
                .map(String::as_str)
                .unwrap_or("<no title>");
            println!("  - {} (score: {:.1}): {}", title, sc.score, reason);
        }
    }

    println!("\n===========================================");
    println!("  Pipeline executed successfully!");
    println!("===========================================\n");

    // ========================================
    // DIFFICULTY CLASSIFIER DEMO
    // ========================================
    println!("\n===========================================");
    println!("  DIFFICULTY CLASSIFIER DEMO");
    println!("===========================================\n");

    use yaatal_search::difficulty::DifficultyClassifier;

    let classifier = DifficultyClassifier::default();

    let test_queries = vec![
        "find Rust tutorials",
        "show me Python docs",
        "what is async/await",
        "compare Rust vs Go for microservices",
        "analyze Python and Rust for data processing",
        "explain the borrow checker in Rust",
    ];

    for query in test_queries {
        let analysis = classifier.analyze(query);
        let stages = classifier.recommended_stages(query);

        println!("Query: \"{}\"", query);
        println!("  Difficulty: {:?}", analysis.difficulty);
        println!("  Score: {:.2}", analysis.score);
        println!("  Intents: {}", analysis.intent_count);
        println!("  Words: {}", analysis.word_count);
        println!("  Hard keywords: {:?}", analysis.hard_keywords);
        println!(
            "  SKIP enrichment: {}, SKIP rerank: {}",
            stages.skip_enrichment, stages.skip_llm_reranking
        );
        println!("  Reason: {}\n", stages.reason);
    }

    println!("===========================================");
    println!("  All examples completed!");
    println!("===========================================\n");

    Ok(())
}
