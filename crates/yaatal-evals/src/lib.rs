//! Evaluation utilities for the Yaatal AI harness.
//!
//! This crate provides simple metrics for evaluating ranking quality.
//! Use these helpers in offline experiments or during A/B testing.

use yaatal_core::ScoredCandidate;

/// Compute mean reciprocal rank (MRR) given a list of ranked items and
/// a set of relevant identifiers. Returns 0.0 if there are no
/// relevant items or if the candidate list is empty.
pub fn mean_reciprocal_rank(rankings: &[ScoredCandidate], relevant_ids: &[String]) -> f32 {
    for (index, item) in rankings.iter().enumerate() {
        if relevant_ids.contains(&item.candidate.id) {
            return 1.0 / (index as f32 + 1.0);
        }
    }
    0.0
}

/// Compute normalized discounted cumulative gain (NDCG) at k. Assumes
/// relevance is binary: items whose identifiers are in `relevant_ids`
/// have relevance 1, others 0. Returns a value between 0.0 and 1.0.
pub fn ndcg_at_k(rankings: &[ScoredCandidate], relevant_ids: &[String], k: usize) -> f32 {
    let k = k.min(rankings.len());
    if k == 0 || relevant_ids.is_empty() {
        return 0.0;
    }
    let mut dcg = 0.0;
    let mut idcg = 0.0;
    for i in 0..k {
        let rank = i + 1;
        let log_denom = (rank as f32 + 1.0).log2();
        if relevant_ids.contains(&rankings[i].candidate.id) {
            dcg += 1.0 / log_denom;
        }
        if i < relevant_ids.len() {
            idcg += 1.0 / ((i + 1) as f32 + 1.0).log2();
        }
    }
    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}
