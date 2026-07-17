mod common;

use std::sync::Arc;

use uuid::Uuid;
use yaatal_edge_turn::{
    Decision, EdgeTurnRequest, MinimindHttpBackend, ToolName, CONTRACT_VERSION,
    MAX_TRANSCRIPT_CHARS,
};

use common::{request, runner, DenyAll};

#[tokio::test]
async fn unknown_product_is_denied_before_policy_dispatch() {
    let (runner, _) = runner(
        r#"{"tool":"studio.switch_product","product_id":"not-current","confidence":0.9}"#,
        None,
    );
    let response = runner.execute(request()).await.expect("turn completes");
    assert_eq!(response.decision, Decision::Deny);
    assert_eq!(response.reason_code, "product_not_in_current_session");
    assert!(response.proposal.is_none());
}

#[tokio::test]
async fn malformed_unknown_and_extra_model_output_is_denied() {
    for output in [
        "not json",
        r#"{"tool":"shell","confidence":1.0}"#,
        r#"{"tool":"none","confidence":1.0,"extra":true}"#,
    ] {
        let (runner, _) = runner(output, None);
        let response = runner.execute(request()).await.expect("turn completes");
        assert_eq!(response.decision, Decision::Deny, "output={output}");
        assert_eq!(response.reason_code, "model_output_invalid");
    }
}

#[tokio::test]
async fn explicit_policy_deny_returns_no_proposal() {
    let (runner, _) = runner(
        r#"{"tool":"studio.switch_product","product_id":"product-1","confidence":0.9}"#,
        Some(Arc::new(DenyAll)),
    );
    let response = runner.execute(request()).await.expect("turn completes");
    assert_eq!(response.decision, Decision::Deny);
    assert_eq!(response.reason_code, "policy_denied");
    assert!(response.proposal.is_none());
}

#[test]
fn request_rejects_unknown_fields_and_invalid_confidence() {
    let unknown = format!(
        r#"{{"version":"{}","run_id":"{}","source":"seller_speech","transcript":{{"text":"x","language":"wo","confidence":0.9}},"model_backend":"mock","token":"secret"}}"#,
        CONTRACT_VERSION,
        Uuid::new_v4()
    );
    assert!(serde_json::from_str::<EdgeTurnRequest>(&unknown).is_err());

    let mut invalid = request();
    invalid.transcript.confidence = f64::NAN;
    assert!(invalid.validate().is_err());
}

#[test]
fn request_rejects_oversized_transcript() {
    let mut oversized = request();
    oversized.transcript.text = "x".repeat(MAX_TRANSCRIPT_CHARS + 1);
    assert!(oversized.validate().is_err());

    let mut at_limit = request();
    at_limit.transcript.text = "x".repeat(MAX_TRANSCRIPT_CHARS);
    assert!(at_limit.validate().is_ok());
}

#[test]
fn proposal_tool_names_are_stable() {
    assert_eq!(
        ToolName::UpdatePriceOverlay.as_str(),
        "studio.update_price_overlay"
    );
}

#[test]
fn minimind_backend_accepts_only_loopback_hosts() {
    for url in [
        "http://localhost:8008",
        "http://127.0.0.1:8008/",
        "http://[::1]:8008",
    ] {
        assert!(MinimindHttpBackend::new(url).is_ok(), "url={url}");
    }

    for url in [
        "https://example.com",
        "http://192.168.1.20:8008",
        "http://localhost.example.com:8008",
        "file:///tmp/minimind.sock",
    ] {
        assert!(MinimindHttpBackend::new(url).is_err(), "url={url}");
    }
}
