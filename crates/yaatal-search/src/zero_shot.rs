use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A searchable document for ColBERT-style retrieval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchDocument {
    pub id: String,
    pub text: String,
}

/// A query with ground-truth document IDs for retrieval evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchQuery {
    pub id: String,
    pub text: String,
    pub relevant_doc_ids: Vec<String>,
}

/// Dataset used to run a zero-shot retrieval benchmark.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZeroShotDataset {
    pub documents: Vec<SearchDocument>,
    pub queries: Vec<SearchQuery>,
}

/// A ranked retrieval result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankedHit {
    pub doc_id: String,
    pub score: f32,
}

/// Errors returned by zero-shot benchmark setup or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZeroShotError {
    InvalidTopK(usize),
    EmptyDataset,
    NoLabeledQueries,
    Backend(String),
}

impl Display for ZeroShotError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTopK(value) => write!(f, "top_k must be > 0, got {value}"),
            Self::EmptyDataset => write!(f, "dataset must contain at least one query"),
            Self::NoLabeledQueries => write!(
                f,
                "dataset contains no queries with at least one relevant document"
            ),
            Self::Backend(message) => write!(f, "retriever backend error: {message}"),
        }
    }
}

impl Error for ZeroShotError {}

/// Backend contract for retrieval.
///
/// Implement this trait with a zero-shot ColBERT backend (local service, Python
/// bridge, remote API, etc), then call [`evaluate_zero_shot`] to compute
/// benchmark metrics.
pub trait Retriever {
    fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<RankedHit>, ZeroShotError>;
}

/// Aggregated benchmark metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZeroShotMetrics {
    pub mrr_at_k: f64,
    pub recall_at_k: f64,
    pub ndcg_at_k: f64,
    pub evaluated_queries: usize,
}

/// Evaluate a retriever on a labeled dataset.
///
/// Metrics:
/// - `mrr_at_k`: mean reciprocal rank at `k`
/// - `recall_at_k`: mean recall at `k`
/// - `ndcg_at_k`: mean normalized discounted cumulative gain at `k` (binary relevance)
pub fn evaluate_zero_shot<R: Retriever>(
    retriever: &R,
    dataset: &ZeroShotDataset,
    top_k: usize,
) -> Result<ZeroShotMetrics, ZeroShotError> {
    if top_k == 0 {
        return Err(ZeroShotError::InvalidTopK(top_k));
    }
    if dataset.queries.is_empty() {
        return Err(ZeroShotError::EmptyDataset);
    }

    let mut mrr_sum = 0.0_f64;
    let mut recall_sum = 0.0_f64;
    let mut ndcg_sum = 0.0_f64;
    let mut evaluated_queries = 0_usize;

    for query in &dataset.queries {
        if query.relevant_doc_ids.is_empty() {
            continue;
        }

        let relevant: HashSet<&str> = query.relevant_doc_ids.iter().map(String::as_str).collect();
        let hits = retriever.retrieve(&query.text, top_k)?;

        let mut first_relevant_rank: Option<usize> = None;
        let mut relevant_hits = 0_usize;
        let mut dcg = 0.0_f64;

        for (idx, hit) in hits.iter().take(top_k).enumerate() {
            if relevant.contains(hit.doc_id.as_str()) {
                if first_relevant_rank.is_none() {
                    first_relevant_rank = Some(idx + 1);
                }
                relevant_hits += 1;
                dcg += 1.0 / ((idx + 2) as f64).log2();
            }
        }

        let idcg_limit = relevant.len().min(top_k);
        let mut idcg = 0.0_f64;
        for idx in 0..idcg_limit {
            idcg += 1.0 / ((idx + 2) as f64).log2();
        }

        let reciprocal_rank = first_relevant_rank.map_or(0.0, |rank| 1.0 / rank as f64);
        let recall = relevant_hits as f64 / relevant.len() as f64;
        let ndcg = if idcg > 0.0 { dcg / idcg } else { 0.0 };

        mrr_sum += reciprocal_rank;
        recall_sum += recall;
        ndcg_sum += ndcg;
        evaluated_queries += 1;
    }

    if evaluated_queries == 0 {
        return Err(ZeroShotError::NoLabeledQueries);
    }

    Ok(ZeroShotMetrics {
        mrr_at_k: mrr_sum / evaluated_queries as f64,
        recall_at_k: recall_sum / evaluated_queries as f64,
        ndcg_at_k: ndcg_sum / evaluated_queries as f64,
        evaluated_queries,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockRetriever {
        by_query: HashMap<String, Vec<RankedHit>>,
    }

    impl Retriever for MockRetriever {
        fn retrieve(&self, query: &str, top_k: usize) -> Result<Vec<RankedHit>, ZeroShotError> {
            let hits =
                self.by_query.get(query).cloned().ok_or_else(|| {
                    ZeroShotError::Backend(format!("missing mock query: {query}"))
                })?;
            Ok(hits.into_iter().take(top_k).collect())
        }
    }

    fn assert_approx(actual: f64, expected: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1e-9,
            "expected {expected}, got {actual}, delta {delta}"
        );
    }

    #[test]
    fn computes_metrics_for_perfect_ranking() {
        let dataset = ZeroShotDataset {
            documents: vec![],
            queries: vec![SearchQuery {
                id: "q1".into(),
                text: "find alpha".into(),
                relevant_doc_ids: vec!["doc1".into(), "doc2".into()],
            }],
        };

        let retriever = MockRetriever {
            by_query: HashMap::from([(
                "find alpha".into(),
                vec![
                    RankedHit {
                        doc_id: "doc1".into(),
                        score: 0.9,
                    },
                    RankedHit {
                        doc_id: "doc2".into(),
                        score: 0.8,
                    },
                ],
            )]),
        };

        let metrics = evaluate_zero_shot(&retriever, &dataset, 10).unwrap();

        assert_approx(metrics.mrr_at_k, 1.0);
        assert_approx(metrics.recall_at_k, 1.0);
        assert_approx(metrics.ndcg_at_k, 1.0);
        assert_eq!(metrics.evaluated_queries, 1);
    }

    #[test]
    fn computes_metrics_for_partial_ranking() {
        let dataset = ZeroShotDataset {
            documents: vec![],
            queries: vec![SearchQuery {
                id: "q1".into(),
                text: "find beta".into(),
                relevant_doc_ids: vec!["doc2".into(), "doc4".into()],
            }],
        };

        let retriever = MockRetriever {
            by_query: HashMap::from([(
                "find beta".into(),
                vec![
                    RankedHit {
                        doc_id: "doc8".into(),
                        score: 0.9,
                    },
                    RankedHit {
                        doc_id: "doc2".into(),
                        score: 0.8,
                    },
                    RankedHit {
                        doc_id: "doc6".into(),
                        score: 0.7,
                    },
                ],
            )]),
        };

        let metrics = evaluate_zero_shot(&retriever, &dataset, 3).unwrap();

        assert_approx(metrics.mrr_at_k, 0.5);
        assert_approx(metrics.recall_at_k, 0.5);

        // DCG = 1/log2(2+1), IDCG = 1/log2(2) + 1/log2(3)
        let expected_ndcg = (1.0 / 3.0_f64.log2()) / (1.0 + (1.0 / 3.0_f64.log2()));
        assert_approx(metrics.ndcg_at_k, expected_ndcg);
        assert_eq!(metrics.evaluated_queries, 1);
    }

    #[test]
    fn validates_input() {
        let empty = ZeroShotDataset::default();
        let retriever = MockRetriever {
            by_query: HashMap::new(),
        };

        assert_eq!(
            evaluate_zero_shot(&retriever, &empty, 10).unwrap_err(),
            ZeroShotError::EmptyDataset
        );

        let unlabeled = ZeroShotDataset {
            documents: vec![],
            queries: vec![SearchQuery {
                id: "q".into(),
                text: "x".into(),
                relevant_doc_ids: vec![],
            }],
        };
        assert_eq!(
            evaluate_zero_shot(&retriever, &unlabeled, 10).unwrap_err(),
            ZeroShotError::NoLabeledQueries
        );

        let labeled = ZeroShotDataset {
            documents: vec![],
            queries: vec![SearchQuery {
                id: "q".into(),
                text: "x".into(),
                relevant_doc_ids: vec!["doc1".into()],
            }],
        };
        assert_eq!(
            evaluate_zero_shot(&retriever, &labeled, 0).unwrap_err(),
            ZeroShotError::InvalidTopK(0)
        );
    }
}
