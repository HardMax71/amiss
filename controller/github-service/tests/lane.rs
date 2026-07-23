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

use std::time::Duration;

use amiss_controller::{CheckConclusion, RunFailure};
use amiss_controller_service::{AdmissionRejection, InboxState, WorkOutcome};

use harness::{Harness, LaneCase};

#[test]
fn signed_delivery_reaches_one_pass_and_replay_is_suppressed() {
    let mut harness = Harness::new(LaneCase::Pass, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(harness.conclusion(), Some(CheckConclusion::Pass));
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());

    harness.enqueue();
    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(harness.api.publications().len(), 1);
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn signed_delivery_for_another_target_is_forbidden_before_storage() {
    let harness = Harness::new(LaneCase::Pass, Duration::from_secs(30));

    assert_eq!(
        harness.target_rejection("release"),
        Some(AdmissionRejection::Forbidden)
    );
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn wrong_provider_identity_never_reaches_publication() {
    let mut harness = Harness::new(LaneCase::WrongIdentity, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert!(harness.api.publications().is_empty());
    assert!(matches!(
        harness
            .inbox
            .lock()
            .unwrap()
            .entries()
            .unwrap()
            .first()
            .unwrap()
            .state,
        InboxState::Pending { .. }
    ));
}

#[test]
fn wrong_tree_and_changed_bootstrap_publish_tampered_runtime() {
    for case in [LaneCase::WrongTree, LaneCase::TamperedRuntime] {
        let mut harness = Harness::new(case, Duration::from_secs(30));
        harness.enqueue();

        assert_eq!(harness.work(), WorkOutcome::Processed);
        assert_eq!(
            harness.conclusion(),
            Some(CheckConclusion::Unavailable(RunFailure::TamperedRuntime))
        );
    }
}

#[test]
fn expired_saved_delivery_is_discarded_before_provider_use() {
    let mut harness = Harness::new(LaneCase::Pass, Duration::from_millis(100));
    harness.enqueue();
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert!(harness.api.publications().is_empty());
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn ruleset_revocation_reaches_the_required_check() {
    let mut harness = Harness::new(LaneCase::Revoked, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(
        harness.conclusion(),
        Some(CheckConclusion::Unavailable(
            RunFailure::AuthorizationRevoked
        ))
    );
}

#[test]
fn missing_output_and_timeout_reach_distinct_failures() {
    for (case, failure) in [
        (LaneCase::MissingOutput, RunFailure::MissingOutput),
        (LaneCase::Timeout, RunFailure::Timeout),
    ] {
        let mut harness = Harness::new(case, Duration::from_secs(30));
        harness.enqueue();

        assert_eq!(harness.work(), WorkOutcome::Processed);
        assert_eq!(
            harness.conclusion(),
            Some(CheckConclusion::Unavailable(failure))
        );
    }
}
