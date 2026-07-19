use std::sync::Arc;
use std::time::Instant;

use uuid::Uuid;
use yaatal_audit::{ActionKind, AuditEventBuilder, AuditStore, PolicyVerdict};
use yaatal_core::RequestContext;
use yaatal_policy::tool_policy::ToolPolicy;

use crate::{
    build_prompt, validate_proposal, ContextSource, Decision, EdgeTurnError, EdgeTurnRequest,
    EdgeTurnResponse, ModelProposal, ProposalBackend, ToolName, CONTRACT_VERSION,
};

pub struct EdgeTurnRunner {
    context_source: Arc<dyn ContextSource>,
    backend: Arc<dyn ProposalBackend>,
    store: Arc<dyn AuditStore>,
    policy: Arc<dyn ToolPolicy>,
    actor: String,
}

impl EdgeTurnRunner {
    pub fn new(
        context_source: Arc<dyn ContextSource>,
        backend: Arc<dyn ProposalBackend>,
        store: Arc<dyn AuditStore>,
        policy: Arc<dyn ToolPolicy>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            context_source,
            backend,
            store,
            policy,
            actor: actor.into(),
        }
    }

    pub async fn execute(
        &self,
        request: EdgeTurnRequest,
    ) -> Result<EdgeTurnResponse, EdgeTurnError> {
        request.validate()?;
        let context = self
            .context_source
            .current()
            .await
            .map_err(EdgeTurnError::Context)?;
        let prompt = build_prompt(&request, &context)?;

        let started = Instant::now();
        let model_result = self.backend.propose(&prompt).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let model_action_name = match &model_result {
            Ok(result) => result
                .audit_identity()
                .unwrap_or_else(|| self.backend.name().to_string()),
            Err(_) => self.backend.name().to_string(),
        };
        let (raw_output, model_ok) = match &model_result {
            Ok(result) => (result.output.as_str(), true),
            Err(error) => (error.as_str(), false),
        };
        let mut model_event = AuditEventBuilder::new(
            request.run_id,
            &self.actor,
            ActionKind::ModelCall,
            model_action_name,
        )
        .finish(&prompt, raw_output, model_ok);
        model_event.latency_ms = latency_ms;
        self.store.append(model_event).await?;

        let raw_output = match model_result {
            Ok(result) => result.output,
            Err(_) => {
                return self
                    .finish(
                        request.run_id,
                        Decision::Deny,
                        "model_backend_error",
                        None,
                        "model backend failed",
                        Some(PolicyVerdict::Deny("model backend failed".to_string())),
                    )
                    .await;
            }
        };

        let proposal: ModelProposal = match serde_json::from_str(&raw_output) {
            Ok(proposal) => proposal,
            Err(_) => {
                return self
                    .finish(
                        request.run_id,
                        Decision::Deny,
                        "model_output_invalid",
                        None,
                        &raw_output,
                        Some(PolicyVerdict::Deny(
                            "model output failed strict JSON schema".to_string(),
                        )),
                    )
                    .await;
            }
        };

        if let Err(reason) = validate_proposal(&proposal, &context) {
            return self
                .finish(
                    request.run_id,
                    Decision::Deny,
                    reason,
                    None,
                    &raw_output,
                    Some(PolicyVerdict::Deny(reason.to_string())),
                )
                .await;
        }

        if proposal.tool == ToolName::None {
            return self
                .finish(
                    request.run_id,
                    Decision::Noop,
                    "no_action",
                    None,
                    &raw_output,
                    Some(PolicyVerdict::Allow),
                )
                .await;
        }

        let arguments = serde_json::to_string(&proposal)?;
        let mut ctx = RequestContext::new(request.run_id.to_string());
        ctx.metadata.insert("actor".to_string(), self.actor.clone());
        let verdict = self
            .policy
            .check(&ctx, request.run_id, proposal.tool.as_str(), &arguments)
            .await;

        if verdict.is_deny() {
            let reason = match &verdict {
                PolicyVerdict::Deny(reason) => reason.clone(),
                _ => String::new(),
            };
            return self
                .finish(
                    request.run_id,
                    Decision::Deny,
                    "policy_denied",
                    None,
                    &reason,
                    Some(verdict),
                )
                .await;
        }

        self.finish(
            request.run_id,
            Decision::Allow,
            "validated",
            Some(proposal),
            &arguments,
            Some(verdict),
        )
        .await
    }

    async fn finish(
        &self,
        run_id: Uuid,
        decision: Decision,
        reason_code: &str,
        proposal: Option<ModelProposal>,
        decision_input: &str,
        verdict: Option<PolicyVerdict>,
    ) -> Result<EdgeTurnResponse, EdgeTurnError> {
        let action_name = proposal
            .as_ref()
            .map(|value| value.tool.as_str())
            .unwrap_or("edge-turn-decision");
        let builder =
            AuditEventBuilder::new(run_id, &self.actor, ActionKind::PolicyCheck, action_name);
        let builder = match verdict {
            Some(value) => builder.policy_verdict(value),
            None => builder,
        };
        let event = builder.finish(
            decision_input,
            reason_code,
            matches!(decision, Decision::Allow | Decision::Noop),
        );
        self.store.append(event).await?;
        let audit_event_count = self.store.by_run(run_id).await?.len();
        Ok(EdgeTurnResponse {
            version: CONTRACT_VERSION,
            run_id,
            decision,
            reason_code: reason_code.to_string(),
            proposal,
            audit_event_count,
        })
    }
}
