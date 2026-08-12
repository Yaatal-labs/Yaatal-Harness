//! Custody bridge between a Pi planner and the Harness tool surface.
//!
//! Pi (`@earendil-works/pi-agent-core`) is the agent loop the Harness README
//! says it does not have: *"No general runtime adapters… No model-driven
//! planning."* This crate is the door the charter demands between that loop and
//! anything real.
//!
//! # Why the model never executes anything
//!
//! Pi's own security docs are blunt: it *"does NOT sandbox tool execution by
//! default"*, extensions *"run with the same permissions as Pi"*, and *"real
//! isolation needs to come from the operating system or a virtualization
//! boundary."* A `tool_call` hook that blocks from inside Pi's process is
//! therefore a suggestion, not a control — an agent holding `write` and `bash`
//! can edit whatever governs it.
//!
//! That risk belongs to `pi-coding-agent`, the opinionated CLI. At the
//! `pi-agent-core` layer the built-ins are opt-in factory functions
//! (`createBashTool`, `createWriteTool`, …) and `tools` is optional on
//! `AgentHarnessOptions` — so a harness we construct starts with **no tools at
//! all**. There is nothing to disable; we simply never hand them over.
//!
//! The model therefore cannot execute. It emits a [`ToolIntent`]; this crate
//! resolves it against a [`ToolManifest`] and runs it through
//! `yaatal_tools::AuditedExec`, which checks `ToolPolicy` *before* spawning and
//! writes one `AuditEvent` either way. Same shape as `yaatal-edge-turn`: the
//! model proposes, the Harness disposes.
//!
//! # Roles are configuration
//!
//! An agentic role is an actor id plus a manifest subset. `ToolPolicyGate`
//! already supports per-actor allowlists (`with_actor_allowed`), so adding the
//! Studio, merchant or dev role is a manifest entry and a policy line rather
//! than another integration.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yaatal_audit::AuditStore;
use yaatal_core::RequestContext;
use yaatal_policy::tool_policy::ToolPolicy;
use yaatal_tools::audited_exec::{AuditedExec, ExecError, ExecOutput};

/// The four `pi-agent-core` built-in tool factories. Importing any of them on
/// the Node side is the *only* way a native tool can reach the planner, so the
/// list is kept here and asserted against the shipped entrypoint.
pub const PI_NATIVE_TOOL_FACTORIES: [&str; 4] = [
    "createBashTool",
    "createEditTool",
    "createReadTool",
    "createWriteTool",
];

/// What the planner asked to do. Produced by the Node side from a Pi tool call;
/// carries no execution capability of its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIntent {
    /// Agentic role that proposed this — selects the policy allowlist.
    pub actor: String,
    /// Correlates every event of one planner run in the audit store.
    pub run_id: Uuid,
    /// Manifest key. Never a program path: the model cannot name a binary.
    pub tool: String,
    /// Positional arguments, already parsed from the model's JSON.
    pub args: Vec<String>,
}

/// One bridged tool: the manifest key the model sees, and the command it means.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSpec {
    /// Program to execute. Not model-supplied.
    pub program: String,
    /// Arguments prepended before the model's own, e.g. `["products", "list"]`
    /// so the model names `products_list` rather than assembling a CLI.
    #[serde(default)]
    pub prefix_args: Vec<String>,
    /// One-line description handed to the planner as the tool's docstring.
    pub description: String,
}

/// The complete set of things any planner may propose. Generated once and used
/// twice: to build Pi's tool list, and to resolve an intent back to a command.
/// A tool absent here cannot be named, which is the first of the two gates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolManifest {
    tools: BTreeMap<String, ToolSpec>,
}

impl ToolManifest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_tool(mut self, name: impl Into<String>, spec: ToolSpec) -> Self {
        self.tools.insert(name.into(), spec);
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.get(name)
    }

    /// Tool names, sorted. This is what the Node side turns into Pi's `tools`
    /// array — the planner sees exactly this and nothing else.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// The planner named something not in the manifest. Distinct from a policy
    /// denial: policy answers "you may not", this answers "no such thing".
    #[error("unknown tool `{0}` — not in the manifest")]
    UnknownTool(String),
    #[error("denied by policy: {0}")]
    Denied(String),
    #[error("execution failed: {0}")]
    Exec(String),
    #[error("audit error: {0}")]
    Audit(String),
}

/// Owns the custody path for one agentic role.
///
/// Holds an [`AuditedExec`] rather than its own policy/store/actor/timeout —
/// that type already sequences policy-check-before-spawn, timeout kill, and one
/// `AuditEvent` per invocation. Duplicating its fields here would be a second
/// place for the custody rules to drift.
pub struct PiBridge {
    manifest: ToolManifest,
    exec: AuditedExec,
}

impl PiBridge {
    #[must_use]
    pub fn new(
        manifest: ToolManifest,
        policy: Arc<dyn ToolPolicy>,
        store: Arc<dyn AuditStore>,
        actor: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            manifest,
            exec: AuditedExec::new(policy, store, actor, timeout),
        }
    }

    #[must_use]
    pub fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    /// Resolve an intent to a concrete command without running it.
    ///
    /// Split out from execution so the "can the model name this at all?" gate
    /// is testable on its own, and so a caller can log the resolved command
    /// before anything is spawned.
    pub fn resolve(&self, intent: &ToolIntent) -> Result<(String, Vec<String>), BridgeError> {
        let spec = self
            .manifest
            .get(&intent.tool)
            .ok_or_else(|| BridgeError::UnknownTool(intent.tool.clone()))?;
        let mut args = spec.prefix_args.clone();
        args.extend(intent.args.iter().cloned());
        Ok((spec.program.clone(), args))
    }

    /// Resolve, then run under custody. The two gates in order: the manifest
    /// decides whether the planner could name this at all, then `ToolPolicy`
    /// decides whether this actor may run it — and only then is anything
    /// spawned. Both outcomes leave an `AuditEvent` behind.
    pub async fn dispatch(
        &self,
        ctx: &RequestContext,
        intent: &ToolIntent,
    ) -> Result<ExecOutput, BridgeError> {
        let (program, args) = self.resolve(intent)?;
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        self.exec
            .run(ctx, intent.run_id, &program, &argv)
            .await
            .map_err(|e| match e {
                ExecError::DeniedByPolicy(reason) => BridgeError::Denied(reason),
                ExecError::Audit(inner) => BridgeError::Audit(inner.to_string()),
                other => BridgeError::Exec(other.to_string()),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaatal_audit::PolicyVerdict;

    fn manifest() -> ToolManifest {
        ToolManifest::new().with_tool(
            "products_list",
            ToolSpec {
                program: "yaatal".to_owned(),
                prefix_args: vec!["products".to_owned(), "list".to_owned()],
                description: "List products".to_owned(),
            },
        )
    }

    /// The manifest is the whole vocabulary. A planner that hallucinates a tool
    /// name gets a clean refusal rather than a resolved command.
    #[test]
    fn an_unlisted_tool_cannot_be_named() {
        let bridge = PiBridge::new(
            manifest(),
            Arc::new(AlwaysAllow),
            Arc::new(yaatal_audit::MemoryAuditStore::default()),
            "ops-runner",
            Duration::from_secs(5),
        );
        let err = bridge
            .resolve(&ToolIntent {
                actor: "ops-runner".to_owned(),
                run_id: Uuid::new_v4(),
                tool: "bash".to_owned(),
                args: vec!["rm -rf /".to_owned()],
            })
            .expect_err("bash is not in the manifest");
        assert!(matches!(err, BridgeError::UnknownTool(t) if t == "bash"));
    }

    /// The model supplies arguments, never the program. Prefix args come from
    /// the manifest so a tool name maps to one subcommand, not to a free CLI.
    #[test]
    fn the_model_cannot_choose_the_program() {
        let bridge = PiBridge::new(
            manifest(),
            Arc::new(AlwaysAllow),
            Arc::new(yaatal_audit::MemoryAuditStore::default()),
            "ops-runner",
            Duration::from_secs(5),
        );
        let (program, args) = bridge
            .resolve(&ToolIntent {
                actor: "ops-runner".to_owned(),
                run_id: Uuid::new_v4(),
                tool: "products_list".to_owned(),
                args: vec!["--json".to_owned()],
            })
            .expect("resolves");
        assert_eq!(program, "yaatal");
        assert_eq!(args, vec!["products", "list", "--json"]);
    }

    /// Pi's built-ins are opt-in factories, so the only way one reaches the
    /// planner is if the Node entrypoint imports it. Guard that here: this list
    /// is what the Node-side check greps for.
    #[test]
    fn the_native_tool_factory_list_is_the_ones_that_matter() {
        assert!(PI_NATIVE_TOOL_FACTORIES.contains(&"createBashTool"));
        assert!(PI_NATIVE_TOOL_FACTORIES.contains(&"createWriteTool"));
        assert_eq!(PI_NATIVE_TOOL_FACTORIES.len(), 4);
    }

    /// **The property the whole crate exists for.** A denied intent must leave
    /// no process behind and one audit row saying why. `AuditedExec` checks
    /// policy before spawning, so this asserts the bridge did not route around
    /// it — the failure this guards against is a future refactor that resolves
    /// and spawns first, then asks.
    #[tokio::test]
    async fn a_denied_intent_never_executes_and_is_audited() {
        let store = Arc::new(yaatal_audit::MemoryAuditStore::default());
        let bridge = PiBridge::new(
            // `true` exists on every box and exits 0, so if custody leaked the
            // test would pass on a wrong result rather than error by accident.
            ToolManifest::new().with_tool(
                "noop",
                ToolSpec {
                    program: "true".to_owned(),
                    prefix_args: vec![],
                    description: "does nothing".to_owned(),
                },
            ),
            Arc::new(AlwaysDeny),
            store.clone(),
            "ops-runner",
            Duration::from_secs(5),
        );
        let run_id = Uuid::new_v4();
        let err = bridge
            .dispatch(
                &RequestContext::new("test"),
                &ToolIntent {
                    actor: "ops-runner".to_owned(),
                    run_id,
                    tool: "noop".to_owned(),
                    args: vec![],
                },
            )
            .await
            .expect_err("policy denies");
        assert!(matches!(err, BridgeError::Denied(_)), "got {err:?}");

        let events = store.by_run(run_id).await.expect("audit readable");
        assert_eq!(events.len(), 1, "a denial is still one audited action");
        assert!(
            events[0].policy_verdicts.iter().any(PolicyVerdict::is_deny),
            "the deny verdict must be on the record, got {:?}",
            events[0].policy_verdicts
        );
        assert!(!events[0].success);
    }

    struct AlwaysDeny;
    #[async_trait::async_trait]
    impl ToolPolicy for AlwaysDeny {
        async fn check(
            &self,
            _ctx: &RequestContext,
            _run_id: Uuid,
            tool_name: &str,
            _arguments: &str,
        ) -> PolicyVerdict {
            PolicyVerdict::Deny(format!("{tool_name} not allowed for this actor"))
        }
    }

    struct AlwaysAllow;
    #[async_trait::async_trait]
    impl ToolPolicy for AlwaysAllow {
        async fn check(
            &self,
            _ctx: &RequestContext,
            _run_id: Uuid,
            _tool_name: &str,
            _arguments: &str,
        ) -> PolicyVerdict {
            PolicyVerdict::Allow
        }
    }
}
