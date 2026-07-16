use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use yaatal_audit::{AuditStore, MemoryAuditStore, PolicyVerdict};
use yaatal_core::RequestContext;
use yaatal_edge_turn::{
    ContextSource, EdgeTurnRequest, EdgeTurnRunner, EdgeTurnSource, EngineContext, EngineProduct,
    EngineSession, MockProposalBackend, ModelBackendKind, Transcript, CONTRACT_VERSION,
};
use yaatal_policy::tool_policy::{ToolPolicy, ToolPolicyGate};

pub struct StaticContext;

#[async_trait]
impl ContextSource for StaticContext {
    async fn current(&self) -> Result<EngineContext, String> {
        Ok(context())
    }
}

#[allow(dead_code)]
pub struct DenyAll;

#[async_trait]
impl ToolPolicy for DenyAll {
    async fn check(
        &self,
        _ctx: &RequestContext,
        _run_id: Uuid,
        _tool_name: &str,
        _arguments: &str,
    ) -> PolicyVerdict {
        PolicyVerdict::Deny("test policy deny".to_string())
    }
}

pub fn context() -> EngineContext {
    EngineContext {
        session: EngineSession {
            id: "session-1".to_string(),
            merchant_id: "merchant-1".to_string(),
            product_id: Some("product-1".to_string()),
            status: "active".to_string(),
            started_at: "2026-07-15T00:00:00Z".to_string(),
            ended_at: None,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: None,
        },
        products: vec![EngineProduct {
            id: "product-1".to_string(),
            merchant_id: "merchant-1".to_string(),
            name: "Sac bleu".to_string(),
            description: None,
            price_cents: 10_000,
            price_display: "10 000 FCFA".to_string(),
            discount_price_cents: None,
            discount_price_display: None,
            stock: 7,
            stock_status: "in_stock".to_string(),
            category: "bags".to_string(),
            images: vec![],
            upvotes: 0,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: None,
        }],
    }
}

pub fn request() -> EdgeTurnRequest {
    EdgeTurnRequest {
        version: CONTRACT_VERSION.to_string(),
        run_id: Uuid::new_v4(),
        source: EdgeTurnSource::SellerSpeech,
        transcript: Transcript {
            text: "Le prix est douze mille francs".to_string(),
            language: "fr".to_string(),
            confidence: 0.94,
        },
        model_backend: ModelBackendKind::Mock,
    }
}

pub fn runner(
    output: &str,
    policy_override: Option<Arc<dyn ToolPolicy>>,
) -> (EdgeTurnRunner, Arc<MemoryAuditStore>) {
    let memory = Arc::new(MemoryAuditStore::new());
    let store: Arc<dyn AuditStore> = memory.clone();
    let policy: Arc<dyn ToolPolicy> = policy_override.unwrap_or_else(|| {
        Arc::new(ToolPolicyGate::new(
            [
                "studio.update_price_overlay".to_string(),
                "studio.mark_sold_out_overlay".to_string(),
                "studio.switch_product".to_string(),
            ],
            None,
            store.clone(),
        ))
    });
    (
        EdgeTurnRunner::new(
            Arc::new(StaticContext),
            Arc::new(MockProposalBackend::new(output)),
            store,
            policy,
            "studio:test",
        ),
        memory,
    )
}
