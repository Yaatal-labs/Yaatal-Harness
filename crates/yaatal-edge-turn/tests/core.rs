mod common;

use yaatal_audit::AuditStore;
use yaatal_edge_turn::Decision;

use common::{request, runner};

#[tokio::test]
async fn valid_price_proposal_is_allowed_and_audited_without_raw_speech() {
    let (runner, store) = runner(
        r#"{"tool":"studio.update_price_overlay","product_id":"product-1","price_fcfa":12000,"confidence":0.93}"#,
        None,
    );
    let request = request();
    let run_id = request.run_id;
    let response = runner.execute(request).await.expect("turn completes");

    assert_eq!(response.decision, Decision::Allow);
    assert_eq!(response.reason_code, "validated");
    assert_eq!(response.audit_event_count, 2);
    assert_eq!(
        response.proposal.expect("allowed proposal").price_fcfa,
        Some(12_000)
    );
    let events = store.by_run(run_id).await.expect("audit readable");
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| {
        !serde_json::to_string(event)
            .expect("event serializes")
            .contains("douze mille")
    }));
}

#[tokio::test]
async fn sold_out_and_switch_proposals_are_allowed() {
    for tool in ["studio.mark_sold_out_overlay", "studio.switch_product"] {
        let output = format!(r#"{{"tool":"{tool}","product_id":"product-1","confidence":0.9}}"#);
        let (runner, _) = runner(&output, None);
        let response = runner.execute(request()).await.expect("turn completes");
        assert_eq!(response.decision, Decision::Allow, "tool={tool}");
    }
}

#[tokio::test]
async fn no_action_is_a_successful_noop() {
    let (runner, _) = runner(r#"{"tool":"none","confidence":1.0}"#, None);
    let response = runner.execute(request()).await.expect("turn completes");
    assert_eq!(response.decision, Decision::Noop);
    assert_eq!(response.reason_code, "no_action");
    assert!(response.proposal.is_none());
    assert_eq!(response.audit_event_count, 2);
}
