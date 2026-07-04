//! CONTROL-LOOP slice 2: aggregates over `AuditEvent`s (see `docs/CONTROL-LOOP.md` §3,
//! "METRICS — aggregates over audit events").
//!
//! This is a read path over [`AuditStore`], not a new subsystem — the same way
//! `yaatal-evals`'s `mean_reciprocal_rank` / `ndcg_at_k` are plain functions rather than a
//! service. One struct ([`AuditMetrics`]), one pure function ([`compute`]) that rolls a
//! slice of events into it, and two convenience wrappers that pull the slice from a store
//! for you (per-run, per-time-range).
//!
//! ponytail: this is a **batch** compute over whatever events a caller already has (or
//! whatever a store read returns) — no incremental/streaming aggregation, no caching.
//! That's the L0 ceiling: fine for "how did run X do" / "how did today go", not
//! appropriate if a caller starts wanting live-updating dashboards over a
//! `JsonlAuditStore` with hundreds of thousands of events. Upgrade path: an
//! incrementally-maintained aggregate (updated on each `AuditStore::append`) once a
//! caller's read volume makes recomputing from scratch too slow.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{ActionKind, AuditError, AuditEvent, AuditStore};

/// Rollup over a set of `AuditEvent`s — a run, a time range, or any other slice a caller
/// assembles. All fields are computed fresh from the input slice; nothing here is
/// incremental or cached (see module ponytail note).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AuditMetrics {
    /// Number of events in the slice.
    pub event_count: usize,
    /// Number of events with `success == true`.
    pub success_count: usize,
    /// `success_count / event_count`, or `0.0` for an empty slice (rather than `NaN`).
    pub success_rate: f64,
    /// Sum of every event's `latency_ms`.
    pub total_latency_ms: u64,
    /// 50th-percentile latency (nearest-rank method), `0` for an empty slice.
    pub p50_latency_ms: u64,
    /// 95th-percentile latency (nearest-rank method), `0` for an empty slice.
    pub p95_latency_ms: u64,
    /// Sum of every event's `cost` (events with `cost: None` contribute `0.0`).
    pub total_cost: f64,
    /// Event count grouped by `action_kind`.
    pub by_action_kind: HashMap<ActionKind, usize>,
    /// Event count grouped by `actor`.
    pub by_actor: HashMap<String, usize>,
}

/// Nearest-rank percentile over an already-sorted-ascending slice. `p` is a fraction in
/// `[0.0, 1.0]` (e.g. `0.95` for p95). Returns `0` for an empty slice.
fn percentile(sorted_ascending: &[u64], p: f64) -> u64 {
    if sorted_ascending.is_empty() {
        return 0;
    }
    let rank = (p * sorted_ascending.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted_ascending.len() - 1);
    sorted_ascending[index]
}

/// Compute an [`AuditMetrics`] rollup over `events`. Pure function, no I/O — callers that
/// want a run's or a time range's events pulled from a store should use
/// [`compute_for_run`] / [`compute_in_range`] instead.
pub fn compute(events: &[AuditEvent]) -> AuditMetrics {
    let event_count = events.len();
    if event_count == 0 {
        return AuditMetrics::default();
    }

    let success_count = events.iter().filter(|e| e.success).count();
    let success_rate = success_count as f64 / event_count as f64;

    let mut latencies: Vec<u64> = events.iter().map(|e| e.latency_ms).collect();
    latencies.sort_unstable();
    let total_latency_ms: u64 = latencies.iter().sum();
    let p50_latency_ms = percentile(&latencies, 0.50);
    let p95_latency_ms = percentile(&latencies, 0.95);

    let total_cost: f64 = events.iter().filter_map(|e| e.cost).sum();

    let mut by_action_kind: HashMap<ActionKind, usize> = HashMap::new();
    let mut by_actor: HashMap<String, usize> = HashMap::new();
    for event in events {
        *by_action_kind.entry(event.action_kind).or_insert(0) += 1;
        *by_actor.entry(event.actor.clone()).or_insert(0) += 1;
    }

    AuditMetrics {
        event_count,
        success_count,
        success_rate,
        total_latency_ms,
        p50_latency_ms,
        p95_latency_ms,
        total_cost,
        by_action_kind,
        by_actor,
    }
}

/// Convenience: pull a run's events from `store` and roll them up.
pub async fn compute_for_run(
    store: &dyn AuditStore,
    run_id: Uuid,
) -> Result<AuditMetrics, AuditError> {
    let events = store.by_run(run_id).await?;
    Ok(compute(&events))
}

/// Convenience: pull a time range's events from `store` and roll them up.
pub async fn compute_in_range(
    store: &dyn AuditStore,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<AuditMetrics, AuditError> {
    let events = store.in_range(from, to).await?;
    Ok(compute(&events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuditEventBuilder, MemoryAuditStore};

    fn seed_event(
        run_id: Uuid,
        actor: &str,
        kind: ActionKind,
        cost: f64,
        success: bool,
    ) -> AuditEvent {
        AuditEventBuilder::new(run_id, actor, kind, "action")
            .cost(cost)
            .finish("in", "out", success)
    }

    #[test]
    fn compute_over_empty_slice_is_all_zero() {
        let metrics = compute(&[]);
        assert_eq!(metrics.event_count, 0);
        assert_eq!(metrics.success_rate, 0.0);
        assert_eq!(metrics.p50_latency_ms, 0);
        assert_eq!(metrics.p95_latency_ms, 0);
        assert_eq!(metrics.total_cost, 0.0);
    }

    #[test]
    fn compute_rolls_up_count_success_cost_and_groupings() {
        let run_id = Uuid::new_v4();
        let mut e1 = seed_event(run_id, "engine:cron", ActionKind::ToolCall, 0.01, true);
        e1.latency_ms = 100;
        let mut e2 = seed_event(run_id, "engine:cron", ActionKind::ToolCall, 0.02, false);
        e2.latency_ms = 200;
        let mut e3 = seed_event(run_id, "studio:live", ActionKind::ModelCall, 0.03, true);
        e3.latency_ms = 300;

        let events = vec![e1, e2, e3];
        let metrics = compute(&events);

        assert_eq!(metrics.event_count, 3);
        assert_eq!(metrics.success_count, 2);
        assert!((metrics.success_rate - (2.0 / 3.0)).abs() < 1e-9);
        assert_eq!(metrics.total_latency_ms, 600);
        assert!((metrics.total_cost - 0.06).abs() < 1e-9);

        assert_eq!(metrics.by_action_kind.get(&ActionKind::ToolCall), Some(&2));
        assert_eq!(metrics.by_action_kind.get(&ActionKind::ModelCall), Some(&1));
        assert_eq!(metrics.by_actor.get("engine:cron"), Some(&2));
        assert_eq!(metrics.by_actor.get("studio:live"), Some(&1));
    }

    #[test]
    fn percentile_matches_nearest_rank_on_a_known_set() {
        // 10 values 10..=100 step 10: p50 -> 50 (rank 5), p95 -> 100 (rank 10).
        let sorted: Vec<u64> = (1..=10).map(|n| n * 10).collect();
        assert_eq!(percentile(&sorted, 0.50), 50);
        assert_eq!(percentile(&sorted, 0.95), 100);
        assert_eq!(percentile(&sorted, 1.0), 100);
    }

    #[tokio::test]
    async fn compute_for_run_and_in_range_pull_from_a_seeded_store() {
        let store = MemoryAuditStore::new();
        let run_a = Uuid::new_v4();
        let run_b = Uuid::new_v4();

        store
            .append(seed_event(
                run_a,
                "engine:cron",
                ActionKind::ToolCall,
                0.1,
                true,
            ))
            .await
            .expect("append a1");
        store
            .append(seed_event(
                run_a,
                "engine:cron",
                ActionKind::ModelCall,
                0.2,
                true,
            ))
            .await
            .expect("append a2");
        store
            .append(seed_event(
                run_b,
                "studio:live",
                ActionKind::ToolCall,
                0.3,
                false,
            ))
            .await
            .expect("append b1");

        let run_a_metrics = compute_for_run(&store, run_a)
            .await
            .expect("compute_for_run");
        assert_eq!(run_a_metrics.event_count, 2);
        assert!((run_a_metrics.total_cost - 0.3).abs() < 1e-9);

        let now = Utc::now();
        let earlier = now - chrono::Duration::minutes(5);
        let later = now + chrono::Duration::minutes(5);
        let range_metrics = compute_in_range(&store, earlier, later)
            .await
            .expect("compute_in_range");
        assert_eq!(range_metrics.event_count, 3);
        assert_eq!(range_metrics.success_count, 2);
    }
}
