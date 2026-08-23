//! Governed edge-turn bridge between Studio, a local proposal model, and Engine context.
//!
//! The model only proposes a local Studio action. This crate owns the trust boundary:
//! request parsing, Engine product lookup, strict proposal validation, tool policy, and
//! digest-only audit. No order, inventory, payment, delivery, or profile mutation is
//! available through this contract.

mod backends;
mod contract;
mod runner;
mod server;

pub use backends::{
    ContextSource, HttpEngineContextSource, MinimindHttpBackend, MockProposalBackend,
    ProposalBackend, ProposalResult,
};
pub use contract::{
    Decision, EdgeTurnRequest, EdgeTurnResponse, EdgeTurnSource, EngineContext, EngineProduct,
    EngineSession, ModelBackendKind, ModelProposal, ToolName, Transcript, CONTRACT_VERSION,
    MAX_PRICE_FCFA, MAX_TRANSCRIPT_CHARS,
};
pub use runner::EdgeTurnRunner;
pub use server::{
    serve, ServerConfig, ServerState, DEFAULT_EDGE_TURN_PORT, DEFAULT_ENGINE_API_URL,
};

use yaatal_audit::AuditError;

#[derive(Debug, thiserror::Error)]
pub enum EdgeTurnError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("Engine context error: {0}")]
    Context(String),
    #[error("audit error: {0}")]
    Audit(#[from] AuditError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub(crate) use contract::{build_prompt, validate_proposal};
