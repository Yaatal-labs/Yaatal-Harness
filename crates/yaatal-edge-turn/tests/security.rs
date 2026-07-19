mod common;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;
use yaatal_audit::AuditStore;
use yaatal_edge_turn::{
    Decision, EdgeTurnRequest, MinimindHttpBackend, ProposalBackend, ProposalResult, ToolName,
    CONTRACT_VERSION, MAX_TRANSCRIPT_CHARS,
};

use common::{request, runner, runner_with_backend, DenyAll};

struct ResultBackend {
    result: ProposalResult,
}

#[async_trait]
impl ProposalBackend for ResultBackend {
    fn name(&self) -> &str {
        "minimind-o"
    }

    async fn propose(&self, _prompt: &str) -> Result<ProposalResult, String> {
        Ok(self.result.clone())
    }
}

async fn serve_minimind_response(body: String) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener binds");
    let address = listener.local_addr().expect("listener has address");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("request accepted");
        let mut request_bytes = vec![0_u8; 8192];
        let _ = socket
            .read(&mut request_bytes)
            .await
            .expect("request readable");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("response writable");
    });
    (format!("http://{address}"), task)
}

async fn audited_model_action(result: ProposalResult) -> String {
    let (runner, store) = runner_with_backend(Arc::new(ResultBackend { result }), None);
    let request = request();
    let run_id = request.run_id;
    runner.execute(request).await.expect("turn completes");
    store
        .by_run(run_id)
        .await
        .expect("audit readable")
        .into_iter()
        .next()
        .expect("model event recorded")
        .action_name
}

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

#[tokio::test]
async fn backend_dynamic_identity_reaches_model_audit() {
    let action = audited_model_action(ProposalResult {
        output: r#"{"tool":"none","confidence":1.0}"#.to_string(),
        runner_model: Some("minimind.o_768".to_string()),
        runner_checkpoint: Some("llm_768-v1".to_string()),
    })
    .await;

    assert_eq!(action, "minimind.o_768@llm_768-v1");
}

#[tokio::test]
async fn minimind_http_response_preserves_valid_runner_identity() {
    let (url, server) = serve_minimind_response(
        r#"{"output":"{\"tool\":\"none\",\"confidence\":1.0}","model":"minimind.o_768","checkpoint":"llm_768-v1"}"#
            .to_string(),
    )
    .await;
    let backend = MinimindHttpBackend::new(url).expect("backend builds");

    let result = backend.propose("prompt").await.expect("proposal succeeds");
    server.await.expect("server completes");

    assert_eq!(result.runner_model.as_deref(), Some("minimind.o_768"));
    assert_eq!(result.runner_checkpoint.as_deref(), Some("llm_768-v1"));
}

#[tokio::test]
async fn minimind_legacy_response_falls_back_to_stable_backend_name() {
    let (url, server) = serve_minimind_response(
        r#"{"output":"{\"tool\":\"none\",\"confidence\":1.0}"}"#.to_string(),
    )
    .await;
    let backend = Arc::new(MinimindHttpBackend::new(url).expect("backend builds"));
    assert_eq!(backend.name(), "minimind-o");
    let (runner, store) = runner_with_backend(backend, None);
    let request = request();
    let run_id = request.run_id;

    runner.execute(request).await.expect("turn completes");
    server.await.expect("server completes");
    let events = store.by_run(run_id).await.expect("audit readable");

    assert_eq!(events[0].action_name, "minimind-o");
}

#[tokio::test]
async fn unsafe_or_partial_runner_identity_falls_back_without_leaking() {
    let seller_speech = "Le prix est douze mille francs";
    let too_long = "a".repeat(97);
    let cases = [
        (Some("valid\r\ninjected"), Some("checkpoint")),
        (Some(seller_speech), Some("checkpoint")),
        (Some("C:\\private\\checkpoint.pth"), Some("checkpoint")),
        (Some("model;drop"), Some("checkpoint")),
        (Some(too_long.as_str()), Some("checkpoint")),
        (Some(""), Some("checkpoint")),
        (Some("   "), Some("checkpoint")),
        (Some("valid-model"), None),
        (None, Some("valid-checkpoint")),
    ];

    for (model, checkpoint) in cases {
        let action = audited_model_action(ProposalResult {
            output: r#"{"tool":"none","confidence":1.0}"#.to_string(),
            runner_model: model.map(str::to_string),
            runner_checkpoint: checkpoint.map(str::to_string),
        })
        .await;
        assert_eq!(
            action, "minimind-o",
            "model={model:?} checkpoint={checkpoint:?}"
        );
        assert!(!action.contains(seller_speech));
        assert!(!action.contains('\\'));
        assert!(!action.contains('\n'));
        assert!(!action.contains(';'));
    }
}
