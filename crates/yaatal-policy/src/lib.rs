//! Policy enforcement implementations.
//!
//! This crate defines concrete `PolicyEngine` implementations that
//! apply product or regulatory constraints to ranked items. You can
//! implement policies such as safe content filtering, diversity
//! requirements or business rules here.
//!
//! ## Modules
//!
//! - [`speaker_aware`]: Speaker-aware access control policy

pub mod speaker_aware;

use async_trait::async_trait;
use tracing::info;
use yaatal_core::{HarnessError, PolicyEngine, PolicyResult, RequestContext, ScoredCandidate};

/// A permissive policy that allows all items. Useful for testing and
/// as a fallback when no constraints should be applied.
pub struct AllowAllPolicy;

#[async_trait]
impl PolicyEngine for AllowAllPolicy {
    async fn evaluate(
        &self,
        ctx: &RequestContext,
        items: Vec<ScoredCandidate>,
    ) -> Result<PolicyResult, HarnessError> {
        info!(request_id = %ctx.request_id, count = items.len(), "allow_all_policy");
        Ok(PolicyResult {
            allowed: items,
            denied: vec![],
        })
    }
}

/// A simple policy that denies any item whose identifier contains the
/// substring "blocked". Demonstrates how to attach a denial reason.
pub struct SimpleFilterPolicy;

#[async_trait]
impl PolicyEngine for SimpleFilterPolicy {
    async fn evaluate(
        &self,
        ctx: &RequestContext,
        items: Vec<ScoredCandidate>,
    ) -> Result<PolicyResult, HarnessError> {
        info!(request_id = %ctx.request_id, count = items.len(), "simple_filter_policy");
        let mut allowed = Vec::new();
        let mut denied = Vec::new();
        for item in items {
            if item.candidate.id.contains("blocked") {
                denied.push((item, "contains blocked keyword".into()));
            } else {
                allowed.push(item);
            }
        }
        Ok(PolicyResult { allowed, denied })
    }
}
