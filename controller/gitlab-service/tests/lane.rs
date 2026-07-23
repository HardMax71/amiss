#![expect(
    clippy::unwrap_used,
    reason = "fixed provider-lane fixtures must fail loudly"
)]

#[path = "lane/harness.rs"]
mod harness;
#[path = "lane/provider.rs"]
mod provider;
#[path = "lane/repositories.rs"]
mod repositories;

use axum::http::StatusCode;
use serde_json::json;

use harness::{Harness, LaneCase};
use provider::sign;

#[tokio::test]
async fn signed_policy_job_passes_once_and_same_jti_replay_cannot_pass() {
    let harness = Harness::new(LaneCase::Pass);

    assert_eq!(harness.request().await, StatusCode::NO_CONTENT);
    assert_eq!(harness.api.calls(), 3);
    assert_eq!(harness.request().await, StatusCode::PRECONDITION_FAILED);
    assert_eq!(harness.api.calls(), 3);
}

#[tokio::test]
async fn definitive_engine_and_runtime_failures_never_return_success() {
    for case in [
        LaneCase::Block,
        LaneCase::MissingOutput,
        LaneCase::Timeout,
        LaneCase::WrongTree,
        LaneCase::TamperedBootstrap,
    ] {
        let harness = Harness::new(case);
        assert_eq!(harness.request().await, StatusCode::PRECONDITION_FAILED);
        assert_eq!(harness.api.calls(), 3);
    }
}

#[tokio::test]
async fn policy_runner_expiry_and_request_identity_fail_closed() {
    let wrong_policy = Harness::new(LaneCase::Pass);
    let mut claims = wrong_policy.claims();
    *claims.pointer_mut("/job_config/url").unwrap() =
        json!("https://gitlab.example/project/.gitlab-ci.yml");
    assert_eq!(
        wrong_policy
            .request_with(&sign(&claims), br#"{"merge_request_iid":42}"#)
            .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(wrong_policy.api.calls(), 0);

    let wrong_runner = Harness::new(LaneCase::Pass);
    let mut claims = wrong_runner.claims();
    *claims.get_mut("runner_id").unwrap() = json!("88");
    *claims.get_mut("runner_environment").unwrap() = json!("self-hosted");
    assert_eq!(
        wrong_runner
            .request_with(&sign(&claims), br#"{"merge_request_iid":42}"#)
            .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(wrong_runner.api.calls(), 0);

    let expired = Harness::new(LaneCase::Pass);
    let mut claims = expired.claims();
    let now = claims.get("iat").unwrap().as_u64().unwrap();
    *claims.get_mut("iat").unwrap() = json!(now - 600);
    *claims.get_mut("nbf").unwrap() = json!(now - 601);
    *claims.get_mut("exp").unwrap() = json!(now - 5);
    assert_eq!(
        expired
            .request_with(&sign(&claims), br#"{"merge_request_iid":42}"#)
            .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(expired.api.calls(), 0);

    let wrong_merge_request = Harness::new(LaneCase::Pass);
    assert_eq!(
        wrong_merge_request
            .request_with(
                &sign(&wrong_merge_request.claims()),
                br#"{"merge_request_iid":43}"#,
            )
            .await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(wrong_merge_request.api.calls(), 1);

    let wrong_gate = Harness::new(LaneCase::Pass);
    let mut claims = wrong_gate.claims();
    *claims.get_mut("sha").unwrap() = json!("dddddddddddddddddddddddddddddddddddddddd");
    assert_eq!(
        wrong_gate
            .request_with(&sign(&claims), br#"{"merge_request_iid":42}"#)
            .await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(wrong_gate.api.calls(), 1);
}

#[tokio::test]
async fn fetched_parent_and_final_authority_changes_fail_the_policy_job() {
    for (case, calls) in [
        (LaneCase::WrongParents, 1),
        (LaneCase::FinalPolicyRevoked, 3),
        (LaneCase::FinalGateChanged, 3),
    ] {
        let harness = Harness::new(case);
        assert_eq!(harness.request().await, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(harness.api.calls(), calls);
    }
}
