//! Core pipeline traits adapted from xai-org/x-algorithm (Apache-2.0).
//!
//! The traits are generic enough to support both ranking candidates and
//! ingestion support records while keeping the default pipeline social-feed
//! focused.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_STAGE_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_SIDE_EFFECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Trait for items with a unique string identifier.
/// Implemented by ranking candidates and ingestion support records to enable
/// generic deduplication and tracking across pipeline stages.
pub trait Identifiable {
    /// Returns the unique identifier for this item.
    fn id(&self) -> &str;
}

/// Classifies pipeline errors for retry and circuit-breaker decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Network-level failure (DNS, TCP, TLS). Retryable.
    Network,
    /// Stage exceeded its timeout. Retryable.
    Timeout,
    /// Malformed data from source or serialization failure.
    Parse,
    /// Bug or invariant violation inside the pipeline.
    Internal,
}

impl ErrorKind {
    /// Returns `true` for `Network` and `Timeout` — the two kinds
    /// that a circuit breaker should count toward tripping.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Network | Self::Timeout)
    }
}

/// A position-indexed bitmap indicating which candidates a filter wants to keep.
///
/// Enables O(1) composition: multiple bitmaps can be combined with bitwise AND
/// before materializing the kept/removed split, avoiding per-filter Vec clones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterBitmap {
    /// `kept[i] == true` means candidate at position `i` passes the filter.
    pub kept: Vec<bool>,
}

impl FilterBitmap {
    /// Creates a bitmap that keeps all candidates.
    pub fn keep_all(len: usize) -> Self {
        Self {
            kept: vec![true; len],
        }
    }

    /// Creates a bitmap from a pre-computed keep flags vector.
    pub fn from_keep_flags(kept: Vec<bool>) -> Self {
        Self { kept }
    }

    /// Number of candidates marked as kept.
    pub fn kept_count(&self) -> usize {
        self.kept.iter().filter(|keep| **keep).count()
    }

    /// Number of candidates marked as removed.
    pub fn removed_count(&self) -> usize {
        self.kept.iter().filter(|keep| !**keep).count()
    }

    /// Combines this bitmap with another via logical AND.
    /// Both bitmaps must have the same length.
    pub fn intersect(&self, other: &Self) -> Result<Self, FeedError> {
        if self.kept.len() != other.kept.len() {
            return Err(FeedError::with_kind(
                ErrorKind::Internal,
                "Filter",
                "FilterBitmap",
                format!(
                    "bitmap length mismatch: left={}, right={}",
                    self.kept.len(),
                    other.kept.len(),
                ),
            ));
        }
        let kept = self
            .kept
            .iter()
            .zip(other.kept.iter())
            .map(|(a, b)| *a && *b)
            .collect();
        Ok(Self { kept })
    }

    /// Splits candidates into (kept, removed) based on this bitmap.
    pub fn partition<C: Clone>(&self, candidates: &[C]) -> Result<(Vec<C>, Vec<C>), FeedError> {
        if self.kept.len() != candidates.len() {
            return Err(FeedError::with_kind(
                ErrorKind::Internal,
                "Filter",
                "FilterBitmap",
                format!(
                    "bitmap length mismatch: kept={}, candidates={}",
                    self.kept.len(),
                    candidates.len(),
                ),
            ));
        }

        let mut kept = Vec::with_capacity(self.kept_count());
        let mut removed = Vec::with_capacity(self.removed_count());
        for (candidate, keep) in candidates.iter().zip(self.kept.iter()) {
            if *keep {
                kept.push(candidate.clone());
            } else {
                removed.push(candidate.clone());
            }
        }

        Ok((kept, removed))
    }
}

#[deprecated(
    note = "Use FilterBitmap instead — FilterResult is a type alias that will be removed."
)]
/// Deprecated alias for backward compatibility.
pub type FilterResult = FilterBitmap;

// ─── Source ────────────────────────────────────────────────────────

/// Fetches candidates from a data source.
/// Multiple sources run in parallel (e.g., following + discovery).
#[async_trait]
pub trait Source<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    async fn candidates(&self, query: &Q) -> Result<Vec<C>, FeedError>;

    fn name(&self) -> &'static str;
}

// ─── QueryHydrator ─────────────────────────────────────────────────

/// Enriches the feed query with additional context before sourcing.
/// Runs in parallel. E.g., fetch user's engagement history, language prefs.
#[async_trait]
pub trait QueryHydrator<Q>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    async fn hydrate(&self, query: &Q) -> Result<Q, FeedError>;

    fn update(&self, query: &mut Q, hydrated: Q);

    fn name(&self) -> &'static str;
}

// ─── Hydrator ──────────────────────────────────────────────────────

/// Enriches candidates with additional data (author info, voice metadata, etc.).
/// Runs in parallel. Must return same number of candidates in same order.
#[async_trait]
pub trait Hydrator<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    async fn hydrate(&self, query: &Q, candidates: &[C]) -> Result<Vec<C>, FeedError>;

    fn update(&self, candidate: &mut C, hydrated: C);

    fn update_all(&self, candidates: &mut [C], hydrated: Vec<C>) {
        for (candidate, hydrated_candidate) in candidates.iter_mut().zip(hydrated) {
            self.update(candidate, hydrated_candidate);
        }
    }

    fn name(&self) -> &'static str;
}

// ─── Filter ────────────────────────────────────────────────────────

/// Returns a bitmap describing which candidates should be kept or removed.
#[async_trait]
pub trait Filter<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    async fn filter(&self, query: &Q, candidates: &[C]) -> Result<FilterBitmap, FeedError>;

    fn name(&self) -> &'static str;
}

pub trait Deduplicator<C>: Send + Sync
where
    C: Clone + Send + Sync + 'static,
{
    fn is_duplicate(&self, candidate: &C) -> bool;
}

// ─── Scorer ────────────────────────────────────────────────────────

/// Scores candidates. Runs sequentially (order matters — ML predictions first,
/// then weighted combination, then diversity adjustments).
/// Must return same number of candidates in same order.
#[async_trait]
pub trait Scorer<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    async fn score(&self, query: &Q, candidates: &[C]) -> Result<Vec<C>, FeedError>;

    fn update(&self, candidate: &mut C, scored: C);

    fn update_all(&self, candidates: &mut [C], scored: Vec<C>) {
        for (candidate, scored_candidate) in candidates.iter_mut().zip(scored) {
            self.update(candidate, scored_candidate);
        }
    }

    fn name(&self) -> &'static str;
}

// ─── Selector ──────────────────────────────────────────────────────

/// Sorts and truncates candidates. Typically: sort by score desc, take top K.
#[async_trait]
pub trait Selector<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    async fn select(&self, _query: &Q, candidates: Vec<C>) -> Vec<C> {
        let mut sorted = self.sort(candidates);
        if let Some(limit) = self.size() {
            sorted.truncate(limit);
        }
        sorted
    }

    fn enable(&self, _query: &Q) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_STAGE_TIMEOUT
    }

    fn score(&self, candidate: &C) -> f64;

    fn sort(&self, candidates: Vec<C>) -> Vec<C> {
        let mut sorted = candidates;
        sorted.sort_by(|left, right| {
            self.score(right)
                .partial_cmp(&self.score(left))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted
    }

    fn size(&self) -> Option<usize> {
        None
    }

    fn name(&self) -> &'static str;
}

// ─── Side Effects ──────────────────────────────────────────────────

/// Input passed to side effects after candidate selection.
#[derive(Clone)]
pub struct SideEffectInput<Q, C> {
    pub query: Arc<Q>,
    pub selected_candidates: Vec<C>,
}

/// A side effect that **must succeed** before the pipeline returns.
/// Examples: persisting "served" IDs, updating user state.
/// Failures are logged but do not abort the pipeline.
#[async_trait]
pub trait CriticalSideEffect<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Arc<Q>) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_SIDE_EFFECT_TIMEOUT
    }

    async fn run(&self, input: Arc<SideEffectInput<Q, C>>) -> Result<(), FeedError>;

    fn name(&self) -> &'static str;
}

/// A fire-and-forget side effect spawned on a background task.
/// Examples: analytics events, cache warming, PostHog tracking.
/// Failures are logged as warnings but never block the response.
#[async_trait]
pub trait BestEffortSideEffect<Q, C>: Send + Sync
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    fn enable(&self, _query: &Arc<Q>) -> bool {
        true
    }

    fn timeout(&self) -> Duration {
        DEFAULT_SIDE_EFFECT_TIMEOUT
    }

    async fn run(&self, input: Arc<SideEffectInput<Q, C>>) -> Result<(), FeedError>;

    fn name(&self) -> &'static str;
}

// ─── Error ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FeedError {
    pub kind: ErrorKind,
    pub stage: String,
    pub component: String,
    pub message: String,
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{:?} {}::{}] {}",
            self.kind, self.stage, self.component, self.message
        )
    }
}

impl std::error::Error for FeedError {}

impl FeedError {
    /// Creates a new `FeedError` with `ErrorKind::Internal`.
    pub fn new(
        stage: impl Into<String>,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::with_kind(ErrorKind::Internal, stage, component, message)
    }

    /// Creates a new `FeedError` with an explicit `ErrorKind`.
    pub fn with_kind(
        kind: ErrorKind,
        stage: impl Into<String>,
        component: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            stage: stage.into(),
            component: component.into(),
            message: message.into(),
        }
    }
}
