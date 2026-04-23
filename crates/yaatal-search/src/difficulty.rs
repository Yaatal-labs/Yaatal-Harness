//! Difficulty-aware query classification for pipeline routing.
//!
//! This module implements a lexical analyzer that scores query difficulty
//! to enable selective pipeline execution - simple queries skip expensive
//! stages, complex queries get the full treatment.
//!
//! Based on LocalHost Router's 3-Tier Hybrid approach:
//! - Tier 1 (Easy): Pure retrieval + policy
//! - Tier 2 (Medium): Retrieval + semantic validation + policy
//! - Tier 3 (Hard): Full pipeline with enrichment + LLM reranking
//!
//! ## Example
//!
//! ```rust,ignore
//! use yaatal_search::difficulty::{DifficultyClassifier, QueryDifficulty};
//!
//! let classifier = DifficultyClassifier::default();
//! let difficulty = classifier.classify("find Rust tutorials");  // Easy
//! let difficulty = classifier.classify("compare Python vs Rust async performance");  // Hard
//! ```

use std::{future::Future, pin::Pin};
use yaatal_core::RequestContext;

/// Query difficulty levels for pipeline routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryDifficulty {
    /// Simple query: single intent, common keywords
    /// → Skip enrichment, skip LLM reranking
    Easy,
    /// Medium query: single intent but requires semantic validation
    /// → Skip enrichment, use LLM reranking only if needed
    Medium,
    /// Complex query: multi-intent, rare keywords, or requires enrichment
    /// → Full pipeline: enrich + LLM reranking + policy
    Hard,
}

/// Configuration for difficulty classification.
#[derive(Debug, Clone)]
pub struct DifficultyConfig {
    /// Keywords that immediately trigger Hard classification.
    pub hard_keywords: Vec<String>,
    /// Keywords that immediately trigger Easy classification.
    pub easy_keywords: Vec<String>,
    /// Conjunctions that indicate multi-intent (Hard) queries.
    pub multi_intent_indicators: Vec<String>,
    /// Score threshold above which query is classified as Hard.
    pub hard_threshold: f32,
}

impl Default for DifficultyConfig {
    fn default() -> Self {
        Self {
            // Keywords requiring complex reasoning, specific expertise, or multiple tools
            hard_keywords: vec![
                // Complex reasoning
                "compare",
                "versus",
                "vs",
                "difference between",
                "analyze",
                "evaluate",
                "assess",
                "tradeoff",
                "pros and cons",
                // Multi-tool indicators
                "and",
                "also",
                "plus",
                "both",
                "together",
                // Technical depth
                "optimize",
                "benchmark",
                "performance",
                "latency",
                "throughput",
                // Research queries
                "research",
                "latest",
                "trends",
                "state of",
                // Specific expertise
                "advanced",
                "deep dive",
                "explain",
                "why",
                "how does",
            ]
            .into_iter()
            .map(String::from)
            .collect(),

            // Keywords indicating simple lookup/intent
            easy_keywords: vec![
                "find",
                "show",
                "list",
                "get",
                "what is",
                "who is",
                "where is",
                "when did",
                "define",
                "lookup",
                "search for",
                "look up",
                "retrieve",
            ]
            .into_iter()
            .map(String::from)
            .collect(),

            // Conjunctions indicating multiple intents
            multi_intent_indicators: vec![
                " and ",
                " also ",
                " plus ",
                " & ",
                " and also ",
                " as well as ",
                " both ",
                " together with ",
            ]
            .into_iter()
            .map(String::from)
            .collect(),

            // Score threshold for Hard classification
            hard_threshold: 0.5,
        }
    }
}

/// Difficulty classifier for query routing.
///
/// Analyzes query text to determine pipeline complexity,
/// enabling selective execution of expensive stages.
pub struct DifficultyClassifier {
    config: DifficultyConfig,
}

impl Default for DifficultyClassifier {
    fn default() -> Self {
        Self::new(DifficultyConfig::default())
    }
}

impl DifficultyClassifier {
    /// Create a new classifier with custom configuration.
    pub fn new(config: DifficultyConfig) -> Self {
        Self { config }
    }

    /// Classify a query's difficulty.
    pub fn classify(&self, query: &str) -> QueryDifficulty {
        let q = query.to_lowercase();
        let score = self.compute_score(&q);
        let hard_count = self.count_hard_keywords(&q);

        if score >= self.config.hard_threshold {
            QueryDifficulty::Hard
        } else if hard_count > 0 || score > 0.2 {
            QueryDifficulty::Medium
        } else {
            QueryDifficulty::Easy
        }
    }

    /// Compute difficulty score from 0.0 (easy) to 1.0 (hard).
    pub fn compute_score(&self, query: &str) -> f32 {
        let q = query.to_lowercase();
        let mut score = 0.0;

        // Multi-intent detection (strong signal for Hard)
        let intent_count = self.count_intents(&q);
        if intent_count > 1 {
            score += 0.3 * intent_count as f32;
        }

        // Hard keyword presence
        let hard_count = self.count_hard_keywords(&q);
        score += (hard_count as f32) * 0.2;

        // Easy keyword presence (reduces score)
        let easy_count = self.count_easy_keywords(&q);
        score -= (easy_count as f32) * 0.1;

        // Query length (longer = more complex)
        let word_count = q.split_whitespace().count();
        if word_count > 10 {
            score += 0.15;
        } else if word_count > 5 {
            score += 0.05;
        }

        // Question mark presence (often simpler factual queries)
        if q.contains('?') && !q.contains("how") && !q.contains("why") {
            score -= 0.1;
        }

        // Clamp score between 0.0 and 1.0
        score.max(0.0).min(1.0)
    }

    /// Count multi-intent indicators in query.
    fn count_intents(&self, query: &str) -> usize {
        let q = query.to_lowercase();
        let mut count = 1; // Base intent

        for indicator in &self.config.multi_intent_indicators {
            if q.contains(indicator.as_str()) {
                count += 1;
                break;
            }
        }

        count
    }

    /// Count hard keywords present in query.
    fn count_hard_keywords(&self, query: &str) -> usize {
        let q = query.to_lowercase();
        self.config
            .hard_keywords
            .iter()
            .filter(|kw| keyword_matches(&q, kw))
            .count()
    }

    /// Count easy keywords present in query.
    fn count_easy_keywords(&self, query: &str) -> usize {
        let q = query.to_lowercase();
        self.config
            .easy_keywords
            .iter()
            .filter(|kw| q.contains(kw.as_str()))
            .count()
    }

    /// Get detailed breakdown of difficulty factors.
    pub fn analyze(&self, query: &str) -> DifficultyAnalysis {
        let q = query.to_lowercase();
        let score = self.compute_score(&q);
        let difficulty = self.classify(&q);

        DifficultyAnalysis {
            difficulty,
            score,
            intent_count: self.count_intents(&q),
            hard_keywords: self
                .config
                .hard_keywords
                .iter()
                .filter(|kw| keyword_matches(&q, kw))
                .cloned()
                .collect(),
            easy_keywords: self
                .config
                .easy_keywords
                .iter()
                .filter(|kw| q.contains(kw.as_str()))
                .cloned()
                .collect(),
            word_count: q.split_whitespace().count(),
            is_question: q.contains('?'),
        }
    }

    /// Recommend which pipeline stages to skip.
    pub fn recommended_stages(&self, query: &str) -> StageRecommendation {
        let difficulty = self.classify(query);

        match difficulty {
            QueryDifficulty::Easy => StageRecommendation {
                skip_enrichment: true,
                skip_llm_reranking: true,
                skip_semantic_validation: true,
                reason: "Simple query - direct retrieval sufficient".to_string(),
            },
            QueryDifficulty::Medium => StageRecommendation {
                skip_enrichment: true,
                skip_llm_reranking: false,
                skip_semantic_validation: false,
                reason: "Medium complexity - LLM reranking recommended".to_string(),
            },
            QueryDifficulty::Hard => StageRecommendation {
                skip_enrichment: false,
                skip_llm_reranking: false,
                skip_semantic_validation: false,
                reason: "Complex query - full pipeline recommended".to_string(),
            },
        }
    }
}

fn keyword_matches(query: &str, keyword: &str) -> bool {
    if keyword.contains(' ') {
        return query.contains(keyword);
    }

    query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == keyword)
}

/// Detailed analysis of query difficulty.
#[derive(Debug, Clone)]
pub struct DifficultyAnalysis {
    pub difficulty: QueryDifficulty,
    pub score: f32,
    pub intent_count: usize,
    pub hard_keywords: Vec<String>,
    pub easy_keywords: Vec<String>,
    pub word_count: usize,
    pub is_question: bool,
}

/// Recommended pipeline stages based on difficulty.
#[derive(Debug, Clone)]
pub struct StageRecommendation {
    pub skip_enrichment: bool,
    pub skip_llm_reranking: bool,
    pub skip_semantic_validation: bool,
    pub reason: String,
}

/// Extension to optimize pipeline execution based on difficulty.
pub trait DifficultyAwarePipeline {
    /// Search with difficulty-aware pipeline routing.
    fn search_optimized<'a>(
        &'a self,
        query: &'a str,
        ctx: &'a RequestContext,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<yaatal_core::PolicyResult, yaatal_core::HarnessError>>
                + Send
                + 'a,
        >,
    >;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easy_queries() {
        let classifier = DifficultyClassifier::default();

        let queries = vec![
            "find Rust tutorials",
            "show me Python docs",
            "what is async/await",
            "list available commands",
            "get help",
        ];

        for query in queries {
            let difficulty = classifier.classify(query);
            assert_eq!(difficulty, QueryDifficulty::Easy, "Failed for: {}", query);
        }
    }

    #[test]
    fn test_hard_queries() {
        let classifier = DifficultyClassifier::default();

        let queries = vec![
            "compare Rust vs Go for microservices performance",
            "analyze the tradeoff between async and threading",
            "evaluate Python and Rust for data processing",
            "optimize database queries and also implement caching",
            "latest trends in AI and ML and deep learning",
        ];

        for query in queries {
            let difficulty = classifier.classify(query);
            assert_eq!(difficulty, QueryDifficulty::Hard, "Failed for: {}", query);
        }
    }

    #[test]
    fn test_medium_queries() {
        let classifier = DifficultyClassifier::default();

        let queries = vec![
            "explain async/await in Rust",
            "how does the borrow checker work",
            "why is Rust memory safe",
        ];

        for query in queries {
            let difficulty = classifier.classify(query);
            assert!(
                matches!(difficulty, QueryDifficulty::Medium | QueryDifficulty::Hard),
                "Failed for: {} -> {:?}",
                query,
                difficulty
            );
        }
    }

    #[test]
    fn test_score_computation() {
        let classifier = DifficultyClassifier::default();

        // Easy query should have low score
        let easy_score = classifier.compute_score("find tutorials");
        assert!(easy_score < 0.5, "Easy score too high: {}", easy_score);

        // Hard query should have high score
        let hard_score =
            classifier.compute_score("compare Rust vs Python for async performance optimization");
        assert!(hard_score > 0.5, "Hard score too low: {}", hard_score);
    }

    #[test]
    fn test_multi_intent_detection() {
        let classifier = DifficultyClassifier::default();

        // Multi-intent queries
        let multi = classifier.count_intents("show me Rust and also Python");
        assert_eq!(multi, 2);

        // Single intent
        let single = classifier.count_intents("show me Rust");
        assert_eq!(single, 1);
    }

    #[test]
    fn test_stage_recommendation() {
        let classifier = DifficultyClassifier::default();

        // Easy should skip enrichment and reranking
        let rec = classifier.recommended_stages("find tutorials");
        assert!(rec.skip_enrichment);
        assert!(rec.skip_llm_reranking);

        // Hard should use everything
        let rec = classifier.recommended_stages("compare Rust vs Python optimize performance");
        assert!(!rec.skip_enrichment);
        assert!(!rec.skip_llm_reranking);
    }
}
