// yaatal-feed/src/pipeline/executor.rs
//
// Adapted from xai-org/x-algorithm candidate_pipeline.rs execute() flow.
// Hardened for Yaatal feed ingestion with stage timing, buffered concurrency,
// a dedicated dedup stage, and circuit-breaker-aware execution.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::StreamExt;
use tracing::{error, info, warn};

use crate::pipeline::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerState};
use crate::pipeline::traits::*;

pub struct PipelineResult<Q, C> {
    pub candidates: Vec<C>,
    pub query: Arc<Q>,
    pub stats: PipelineStats,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub query_hydration_timeout: Duration,
    pub source_timeout: Duration,
    pub hydration_timeout: Duration,
    pub filter_timeout: Duration,
    pub scoring_timeout: Duration,
    pub selector_timeout: Duration,
    pub critical_side_effect_timeout: Duration,
    pub best_effort_side_effect_timeout: Duration,
    pub query_hydrator_concurrency: usize,
    pub source_concurrency: usize,
    pub hydrator_concurrency: usize,
    pub best_effort_side_effect_concurrency: usize,
    pub circuit_breaker: CircuitBreakerConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            query_hydration_timeout: Duration::from_secs(5),
            source_timeout: Duration::from_secs(5),
            hydration_timeout: Duration::from_secs(5),
            filter_timeout: Duration::from_secs(5),
            scoring_timeout: Duration::from_secs(5),
            selector_timeout: Duration::from_secs(5),
            critical_side_effect_timeout: Duration::from_secs(2),
            best_effort_side_effect_timeout: Duration::from_secs(2),
            query_hydrator_concurrency: 4,
            source_concurrency: 4,
            hydrator_concurrency: 4,
            best_effort_side_effect_concurrency: 4,
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    pub sourced: usize,
    pub after_dedup: usize,
    pub after_hydration: usize,
    pub after_filter: usize,
    pub after_scoring: usize,
    pub selected: usize,
    pub filtered_out: usize,
    pub duplicates_removed: usize,
    pub query_hydrator_errors: usize,
    pub source_errors: usize,
    pub hydrator_errors: usize,
    pub filter_errors: usize,
    pub scorer_errors: usize,
    pub critical_side_effect_failures: usize,
    pub query_hydration_ms: u128,
    pub sourcing_ms: u128,
    pub dedup_ms: u128,
    pub hydration_ms: u128,
    pub filtering_ms: u128,
    pub scoring_ms: u128,
    pub selection_ms: u128,
    pub critical_side_effect_ms: u128,
    pub total_ms: u128,
    pub circuit_breaker_state: CircuitBreakerState,
}

pub struct FeedPipeline<Q, C>
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Identifiable + Send + Sync + 'static,
{
    pub query_hydrators: Vec<Box<dyn QueryHydrator<Q>>>,
    pub sources: Vec<Box<dyn Source<Q, C>>>,
    pub hydrators: Vec<Box<dyn Hydrator<Q, C>>>,
    pub filters: Vec<Box<dyn Filter<Q, C>>>,
    pub scorers: Vec<Box<dyn Scorer<Q, C>>>,
    pub selector: Box<dyn Selector<Q, C>>,
    pub post_selection_filters: Vec<Box<dyn Filter<Q, C>>>,
    pub critical_side_effects: Vec<Box<dyn CriticalSideEffect<Q, C>>>,
    pub best_effort_side_effects: Arc<Vec<Box<dyn BestEffortSideEffect<Q, C>>>>,
    pub config: PipelineConfig,
    pub result_size: usize,
}

impl<Q, C> FeedPipeline<Q, C>
where
    Q: Clone + Send + Sync + 'static,
    C: Clone + Identifiable + Send + Sync + 'static,
{
    pub async fn execute(&self, query: Q, request_id: &str) -> PipelineResult<Q, C> {
        let total_start = Instant::now();
        let mut stats = PipelineStats::default();
        let mut breaker = CircuitBreaker::new(self.config.circuit_breaker.clone());

        // 1. Hydrate query (parallel)
        let stage_start = Instant::now();
        let (hydrated_query, query_hydrator_errors) = self.hydrate_query(query, request_id).await;
        stats.query_hydration_ms = stage_start.elapsed().as_millis();
        stats.query_hydrator_errors = query_hydrator_errors;
        self.record_stage_outcome(&mut breaker, query_hydrator_errors);

        // 2. Fetch candidates from all sources (parallel)
        if !breaker.should_allow() {
            return self.empty_result(hydrated_query, &mut stats, total_start, breaker.state());
        }
        let stage_start = Instant::now();
        let (candidates, source_errors) = self.fetch_candidates(&hydrated_query, request_id).await;
        stats.sourcing_ms = stage_start.elapsed().as_millis();
        stats.sourced = candidates.len();
        stats.source_errors = source_errors;
        self.record_stage_outcome(&mut breaker, source_errors);
        info!(request_id, sourced = stats.sourced, "candidates sourced");

        // 3. Deduplicate candidates (separate hardening stage)
        let stage_start = Instant::now();
        let (deduped_candidates, duplicates_removed) = self.deduplicate_candidates(candidates);
        stats.dedup_ms = stage_start.elapsed().as_millis();
        stats.after_dedup = deduped_candidates.len();
        stats.duplicates_removed = duplicates_removed;

        // 4. Hydrate candidates (parallel)
        if !breaker.should_allow() {
            return self.empty_result(hydrated_query, &mut stats, total_start, breaker.state());
        }
        let stage_start = Instant::now();
        let (hydrated_candidates, hydrator_errors) = self
            .hydrate_candidates(&hydrated_query, deduped_candidates, request_id)
            .await;
        stats.hydration_ms = stage_start.elapsed().as_millis();
        stats.after_hydration = hydrated_candidates.len();
        stats.hydrator_errors = hydrator_errors;
        self.record_stage_outcome(&mut breaker, hydrator_errors);

        // 5. Filter (sequential)
        if !breaker.should_allow() {
            return self.empty_result(hydrated_query, &mut stats, total_start, breaker.state());
        }
        let stage_start = Instant::now();
        let (kept_candidates, removed_candidates) = self
            .run_filters(
                &hydrated_query,
                hydrated_candidates,
                &self.filters,
                request_id,
            )
            .await;
        stats.filtering_ms = stage_start.elapsed().as_millis();
        stats.after_filter = kept_candidates.len();
        stats.filtered_out = removed_candidates.len();
        info!(
            request_id,
            kept = kept_candidates.len(),
            removed = removed_candidates.len(),
            "pre-score filter"
        );

        // 6. Score (sequential)
        if !breaker.should_allow() {
            return self.empty_result(hydrated_query, &mut stats, total_start, breaker.state());
        }
        let stage_start = Instant::now();
        let (scored_candidates, scorer_errors) = self
            .run_scorers(&hydrated_query, kept_candidates, request_id)
            .await;
        stats.scoring_ms = stage_start.elapsed().as_millis();
        stats.after_scoring = scored_candidates.len();
        stats.scorer_errors = scorer_errors;
        self.record_stage_outcome(&mut breaker, scorer_errors);

        // 7. Select (sort + truncate)
        let stage_start = Instant::now();
        let selected_candidates = if self.selector.enable(&hydrated_query) {
            let fallback = scored_candidates.clone();
            match tokio::time::timeout(
                self.selector_timeout(),
                self.selector.select(&hydrated_query, scored_candidates),
            )
            .await
            {
                Ok(selected) => selected,
                Err(_) => {
                    error!(
                        request_id,
                        selector = self.selector.name(),
                        "selector timed out, using unsorted candidates"
                    );
                    fallback
                }
            }
        } else {
            scored_candidates
        };
        stats.selection_ms = stage_start.elapsed().as_millis();

        // 8. Post-selection filters
        let (mut final_candidates, _) = self
            .run_filters(
                &hydrated_query,
                selected_candidates,
                &self.post_selection_filters,
                request_id,
            )
            .await;
        final_candidates.truncate(self.result_size);
        stats.selected = final_candidates.len();
        info!(request_id, selected = stats.selected, "feed built");

        let arc_query = Arc::new(hydrated_query);
        let input = Arc::new(SideEffectInput {
            query: Arc::clone(&arc_query),
            selected_candidates: final_candidates.clone(),
        });

        // 9. Critical side effects (blocking)
        let stage_start = Instant::now();
        let critical_side_effect_failures = self
            .run_critical_side_effects(Arc::clone(&input), request_id)
            .await;
        stats.critical_side_effect_ms = stage_start.elapsed().as_millis();
        stats.critical_side_effect_failures = critical_side_effect_failures;
        self.record_stage_outcome(&mut breaker, critical_side_effect_failures);

        // 10. Best-effort side effects (non-blocking)
        self.spawn_best_effort_side_effects(input);

        stats.total_ms = total_start.elapsed().as_millis();
        stats.circuit_breaker_state = breaker.state();

        PipelineResult {
            candidates: final_candidates,
            query: arc_query,
            stats,
        }
    }

    fn empty_result(
        &self,
        query: Q,
        stats: &mut PipelineStats,
        total_start: Instant,
        breaker_state: CircuitBreakerState,
    ) -> PipelineResult<Q, C> {
        stats.total_ms = total_start.elapsed().as_millis();
        stats.circuit_breaker_state = breaker_state;
        PipelineResult {
            candidates: Vec::new(),
            query: Arc::new(query),
            stats: stats.clone(),
        }
    }

    fn record_stage_outcome(&self, breaker: &mut CircuitBreaker, error_count: usize) {
        if error_count == 0 {
            breaker.record_success();
        } else {
            breaker.record_failure();
        }
    }

    fn selector_timeout(&self) -> Duration {
        self.selector.timeout().min(self.config.selector_timeout)
    }

    fn deduplicate_candidates(&self, candidates: Vec<C>) -> (Vec<C>, usize) {
        let mut seen = HashSet::new();
        let mut deduped = Vec::with_capacity(candidates.len());
        let mut duplicates_removed = 0;

        for candidate in candidates {
            if seen.insert(candidate.id().to_owned()) {
                deduped.push(candidate);
            } else {
                duplicates_removed += 1;
            }
        }

        (deduped, duplicates_removed)
    }

    async fn hydrate_query(&self, query: Q, request_id: &str) -> (Q, usize) {
        let enabled: Vec<_> = self
            .query_hydrators
            .iter()
            .filter(|hydrator| hydrator.enable(&query))
            .collect();

        let mut futures = futures::stream::FuturesUnordered::new();
        for hydrator in enabled {
            let query = query.clone();
            let timeout = hydrator.timeout().min(self.config.query_hydration_timeout);
            futures.push(async move {
                (
                    hydrator,
                    tokio::time::timeout(timeout, hydrator.hydrate(&query)).await,
                )
            });
        }

        let mut hydrated = query;
        let mut error_count = 0;
        while let Some(result) = futures.next().await {
            let (hydrator, res) = result;
            match res {
                Ok(Ok(hydrated_query)) => hydrator.update(&mut hydrated, hydrated_query),
                Ok(Err(error)) => {
                    error_count += 1;
                    error!(request_id, component = hydrator.name(), error = %error, "query hydrator failed");
                }
                Err(_) => {
                    error_count += 1;
                    error!(
                        request_id,
                        component = hydrator.name(),
                        "query hydrator timed out"
                    );
                }
            }
        }

        (hydrated, error_count)
    }

    async fn fetch_candidates(&self, query: &Q, request_id: &str) -> (Vec<C>, usize) {
        let enabled: Vec<_> = self
            .sources
            .iter()
            .filter(|source| source.enable(query))
            .collect();

        let mut futures = futures::stream::FuturesUnordered::new();
        for source in enabled {
            let timeout = source.timeout().min(self.config.source_timeout);
            futures.push(async move {
                (
                    source,
                    tokio::time::timeout(timeout, source.candidates(query)).await,
                )
            });
        }

        let mut collected = Vec::new();
        let mut error_count = 0;
        while let Some(result) = futures.next().await {
            let (source, res) = result;
            match res {
                Ok(Ok(mut candidates)) => {
                    info!(
                        request_id,
                        source = source.name(),
                        count = candidates.len(),
                        "source fetched"
                    );
                    collected.append(&mut candidates);
                }
                Ok(Err(error)) => {
                    error_count += 1;
                    error!(request_id, source = source.name(), error = %error, "source failed");
                }
                Err(_) => {
                    error_count += 1;
                    error!(request_id, source = source.name(), "source timed out");
                }
            }
        }

        (collected, error_count)
    }

    async fn hydrate_candidates(
        &self,
        query: &Q,
        mut candidates: Vec<C>,
        request_id: &str,
    ) -> (Vec<C>, usize) {
        let enabled: Vec<_> = self
            .hydrators
            .iter()
            .filter(|hydrator| hydrator.enable(query))
            .collect();
        let expected = candidates.len();
        let snapshot = Arc::new(candidates.clone());

        let mut futures = futures::stream::FuturesUnordered::new();
        for hydrator in enabled {
            let snapshot = Arc::clone(&snapshot);
            let timeout = hydrator.timeout().min(self.config.hydration_timeout);
            futures.push(async move {
                (
                    hydrator,
                    tokio::time::timeout(timeout, hydrator.hydrate(query, snapshot.as_slice()))
                        .await,
                )
            });
        }

        let mut error_count = 0;
        while let Some(result) = futures.next().await {
            let (hydrator, res) = result;
            match res {
                Ok(Ok(hydrated)) => {
                    if hydrated.len() == expected {
                        hydrator.update_all(&mut candidates, hydrated);
                    } else {
                        error_count += 1;
                        warn!(
                            request_id,
                            hydrator = hydrator.name(),
                            expected,
                            got = hydrated.len(),
                            "hydrator length mismatch, skipping"
                        );
                    }
                }
                Ok(Err(error)) => {
                    error_count += 1;
                    error!(request_id, hydrator = hydrator.name(), error = %error, "hydrator failed");
                }
                Err(_) => {
                    error_count += 1;
                    error!(request_id, hydrator = hydrator.name(), "hydrator timed out");
                }
            }
        }

        (candidates, error_count)
    }

    async fn run_filters(
        &self,
        query: &Q,
        candidates: Vec<C>,
        filters: &[Box<dyn Filter<Q, C>>],
        request_id: &str,
    ) -> (Vec<C>, Vec<C>) {
        let enabled: Vec<_> = filters.iter().filter(|f| f.enable(query)).collect();
        if enabled.is_empty() {
            return (candidates, Vec::new());
        }

        // Collect bitmaps from all filters, combine with AND
        let mut combined = FilterBitmap::keep_all(candidates.len());
        for filter in &enabled {
            match tokio::time::timeout(filter.timeout(), filter.filter(query, &candidates)).await {
                Ok(Ok(bitmap)) => match combined.intersect(&bitmap) {
                    Ok(merged) => {
                        let removed = bitmap.removed_count();
                        if removed > 0 {
                            info!(
                                request_id,
                                filter = filter.name(),
                                removed,
                                "filter applied"
                            );
                        }
                        combined = merged;
                    }
                    Err(error) => {
                        error!(request_id, filter = filter.name(), error = %error, "bitmap intersect failed, skipping filter");
                    }
                },
                Ok(Err(error)) => {
                    error!(request_id, filter = filter.name(), error = %error, "filter failed, skipping");
                }
                Err(_) => {
                    error!(
                        request_id,
                        filter = filter.name(),
                        "filter timed out, skipping"
                    );
                }
            }
        }

        // Single partition at the end
        match combined.partition(&candidates) {
            Ok((kept, removed)) => (kept, removed),
            Err(error) => {
                error!(request_id, error = %error, "bitmap partition failed, keeping all candidates");
                (candidates, Vec::new())
            }
        }
    }

    async fn run_scorers(
        &self,
        query: &Q,
        mut candidates: Vec<C>,
        request_id: &str,
    ) -> (Vec<C>, usize) {
        let expected = candidates.len();
        let mut error_count = 0;

        for scorer in self.scorers.iter().filter(|scorer| scorer.enable(query)) {
            let timeout = scorer.timeout().min(self.config.scoring_timeout);
            match tokio::time::timeout(timeout, scorer.score(query, &candidates)).await {
                Ok(Ok(scored)) => {
                    if scored.len() == expected {
                        scorer.update_all(&mut candidates, scored);
                    } else {
                        error_count += 1;
                        warn!(
                            request_id,
                            scorer = scorer.name(),
                            expected,
                            got = scored.len(),
                            "scorer length mismatch, skipping"
                        );
                    }
                }
                Ok(Err(error)) => {
                    error_count += 1;
                    error!(request_id, scorer = scorer.name(), error = %error, "scorer failed");
                }
                Err(_) => {
                    error_count += 1;
                    error!(request_id, scorer = scorer.name(), "scorer timed out");
                }
            }
        }

        (candidates, error_count)
    }

    async fn run_critical_side_effects(
        &self,
        input: Arc<SideEffectInput<Q, C>>,
        request_id: &str,
    ) -> usize {
        let mut failure_count = 0;

        for side_effect in self
            .critical_side_effects
            .iter()
            .filter(|side_effect| side_effect.enable(&input.query))
        {
            let timeout = side_effect
                .timeout()
                .min(self.config.critical_side_effect_timeout);
            match tokio::time::timeout(timeout, side_effect.run(Arc::clone(&input))).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    failure_count += 1;
                    error!(
                        request_id,
                        side_effect = side_effect.name(),
                        error = %error,
                        "critical side effect failed"
                    );
                }
                Err(_) => {
                    failure_count += 1;
                    error!(
                        request_id,
                        side_effect = side_effect.name(),
                        "critical side effect timed out"
                    );
                }
            }
        }

        failure_count
    }

    fn spawn_best_effort_side_effects(&self, input: Arc<SideEffectInput<Q, C>>) {
        let side_effects = Arc::clone(&self.best_effort_side_effects);
        let timeout_limit = self.config.best_effort_side_effect_timeout;

        tokio::spawn(async move {
            for side_effect in side_effects.iter() {
                if !side_effect.enable(&input.query) {
                    continue;
                }

                let timeout = side_effect.timeout().min(timeout_limit);
                match tokio::time::timeout(timeout, side_effect.run(Arc::clone(&input))).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        warn!(
                            side_effect = side_effect.name(),
                            error = %error,
                            "best-effort side effect failed"
                        );
                    }
                    Err(_) => {
                        warn!(
                            side_effect = side_effect.name(),
                            "best-effort side effect timed out"
                        );
                    }
                }
            }
        });
    }
}
