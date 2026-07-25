#![expect(
    clippy::unwrap_used,
    reason = "fixed provider-lane fixtures must fail loudly"
)]
mod harness;
mod provider;

use std::time::Duration;

use amiss_controller::{CheckConclusion, ProviderError, RunFailure};
use amiss_controller_service::{AdmissionRejection, InboxState, WorkOutcome};

use harness::{Harness, LaneSettings};
use provider::publication_count;

const GITEA_HEADERS: &[&str] = &["x-gitea-signature"];
const FORGEJO_HEADERS: &[&str] = &["x-forgejo-signature", "x-gitea-signature"];

#[test]
fn both_family_headers_reach_one_pass_and_replay_is_suppressed() {
    for (namespace, header) in [("gitea", GITEA_HEADERS), ("forgejo", FORGEJO_HEADERS)] {
        let mut harness = Harness::new(
            LaneSettings::pass(namespace, header),
            Duration::from_secs(30),
        );
        harness.enqueue();

        assert_eq!(harness.work(), WorkOutcome::Processed);
        harness.expect_conclusion(Some(CheckConclusion::Pass));
        assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());

        harness.enqueue();
        assert_eq!(harness.work(), WorkOutcome::Processed);
        assert_eq!(publication_count(&harness.api), 1);
        assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
    }
}

#[test]
fn another_target_is_forbidden_before_storage() {
    let harness = Harness::new(
        LaneSettings::pass("gitea", GITEA_HEADERS),
        Duration::from_secs(30),
    );

    assert_eq!(
        harness.target_rejection("release"),
        Some(AdmissionRejection::Forbidden)
    );
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn wrong_dedicated_reviewer_never_reaches_publication() {
    let mut settings = LaneSettings::pass("forgejo", FORGEJO_HEADERS);
    settings.provider_reviewer_id = 88;
    let mut harness = Harness::new(settings, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(publication_count(&harness.api), 0);
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
    let mut wrong_tree = LaneSettings::pass("gitea", GITEA_HEADERS);
    wrong_tree.wrong_tree = true;
    let mut tampered_runtime = LaneSettings::pass("forgejo", FORGEJO_HEADERS);
    tampered_runtime.tampered_runtime = true;

    for settings in [wrong_tree, tampered_runtime] {
        let mut harness = Harness::new(settings, Duration::from_secs(30));
        harness.enqueue();

        assert_eq!(harness.work(), WorkOutcome::Processed);
        harness.expect_conclusion(Some(CheckConclusion::Unavailable(
            RunFailure::TamperedRuntime,
        )));
    }
}

#[test]
fn publication_failure_is_retried_from_the_staged_result() {
    let mut settings = LaneSettings::pass("forgejo", FORGEJO_HEADERS);
    settings.publish_failures = 1;
    let mut harness = Harness::new(settings, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(publication_count(&harness.api), 0);
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

    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(harness.work(), WorkOutcome::Processed);
    harness.expect_conclusion(Some(CheckConclusion::Pass));
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn expired_saved_delivery_is_discarded_before_provider_use() {
    let mut harness = Harness::new(
        LaneSettings::pass("gitea", GITEA_HEADERS),
        Duration::from_millis(100),
    );
    harness.enqueue();
    std::thread::sleep(Duration::from_millis(150));

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(publication_count(&harness.api), 0);
    assert!(harness.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[test]
fn provider_refresh_failure_remains_retryable() {
    let mut settings = LaneSettings::pass("gitea", GITEA_HEADERS);
    settings.refresh_failure = Some(ProviderError::Unavailable);
    let mut harness = Harness::new(settings, Duration::from_secs(30));
    harness.enqueue();

    assert_eq!(harness.work(), WorkOutcome::Processed);
    assert_eq!(publication_count(&harness.api), 0);
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
