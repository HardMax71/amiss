use amiss_controller::{
    ArtifactReference, ChangeId, CheckConclusion, ProviderError, ProviderRunAttempt, RunFailure,
};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ForgeDialect, ObjectFormat};
use amiss_wire::report::model::{FeedbackAction, FeedbackItem, RepoPath};
use amiss_wire::report::{Disposition, FindingKind};

use super::super::Config;
use super::super::model::{CreateReview, ReviewRecord, UserRecord};
use super::super::publication::{validate_created, validate_publication};
use super::support::{Fixture, oid, provider, reviewer};
use crate::review::ReviewState;

#[test]
fn review_bodies_carry_the_report_feedback_lines() {
    let fixture = Fixture::new("gitea");
    let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
    let mut publication = fixture.publication(snapshot, "evaluation-1", CheckConclusion::Block);
    publication.report = Some(
        amiss_fixtures::captured_report(
            amiss_fixtures::feedback_report(
                0,
                vec![FeedbackItem {
                    action: FeedbackAction::Check,
                    annotation: None,
                    effective_disposition: Disposition::Warn,
                    finding_kinds: vec![FindingKind::DependencyChangedSubjectUnchanged],
                    location_count: std::num::NonZeroU64::new(3).unwrap(),
                    target: Some(RepoPath::Text("docs/guide.md".parse().unwrap())),
                }],
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let artifact_id = "b".repeat(64);
    publication.artifact = Some(ArtifactReference {
        id: artifact_id.clone(),
        locator: format!("https://amiss.example/artifacts/{artifact_id}/report"),
        expires_at_unix_millis: 1_800_000_000_000,
        report_digest: sha256(&publication.report.as_ref().unwrap().bytes),
        semantic_digest: Some(sha256(b"semantic input")),
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    });
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    let mut state = fixture.rest.state.lock().unwrap();
    let body = &state.created[0].body;
    assert!(
        body.contains("\nfindings: fix 0, check 1, existing 0\n"),
        "{body}"
    );
    assert!(
        body.ends_with("- Check target \"docs/guide.md\" affected places 3"),
        "{body}"
    );
    assert!(body.contains("artifact-auth: bearer"), "{body}");
    assert!(
        body.contains(publication.artifact.as_ref().unwrap().locator.as_str()),
        "{body}"
    );
    assert!(
        body.contains(&format!("semantic-input: {}", sha256(b"semantic input"))),
        "{body}"
    );
    assert!(body.contains("/semantic"), "{body}");

    state.data.reviews.last_mut().unwrap().body = body
        .lines()
        .filter(|line| {
            !line.starts_with("semantic-input: ") && !line.starts_with("semantic-input-artifact: ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    drop(state);
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    assert_eq!(fixture.rest.state.lock().unwrap().created.len(), 1);
}

#[test]
fn reviews_are_exact_commit_bound_and_idempotent() {
    let fixture = Fixture::new("gitea");
    let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
    let publication = fixture.publication(snapshot, "evaluation-1", CheckConclusion::Pass);
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    let state = fixture.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 1);
    assert_eq!(state.created[0].event, ReviewState::Approved);
    assert_eq!(state.created[0].commit_id, oid('b'));
    assert!(state.created[0].body.contains("candidate-tree: dddddddd"));
    drop(state);

    let block = fixture.publication(
        fixture.client.refresh(fixture.pull_request()).unwrap(),
        "evaluation-2",
        CheckConclusion::Block,
    );
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &block),
        Ok(())
    );
    let state = fixture.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 2);
    assert_eq!(state.created[1].event, ReviewState::RequestChanges);
    drop(state);

    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    let state = fixture.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 2);
    assert_eq!(state.created[1].event, ReviewState::RequestChanges);
}

#[test]
fn inactive_exact_reviews_are_recreated() {
    for stale in [false, true] {
        let fixture = Fixture::new("gitea");
        let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
        let publication = fixture.publication(snapshot, "evaluation-1", CheckConclusion::Block);
        assert_eq!(
            fixture.client.publish(fixture.pull_request(), &publication),
            Ok(())
        );
        {
            let mut state = fixture.rest.state.lock().unwrap();
            let review = state.data.reviews.last_mut().unwrap();
            review.stale = stale;
            review.dismissed = !stale;
        }

        assert_eq!(
            fixture.client.publish(fixture.pull_request(), &publication),
            Ok(())
        );
        let state = fixture.rest.state.lock().unwrap();
        assert_eq!(state.created.len(), 2);
        let review = state.data.reviews.last().unwrap();
        assert!(!review.stale);
        assert!(!review.dismissed);
    }
}

#[test]
fn conflicting_replay_and_wrong_publication_tree_do_not_publish() {
    let fixture = Fixture::new("forgejo");
    let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
    let publication = fixture.publication(snapshot, "evaluation-1", CheckConclusion::Pass);
    fixture
        .client
        .publish(fixture.pull_request(), &publication)
        .unwrap();
    {
        let mut state = fixture.rest.state.lock().unwrap();
        let latest = state.data.reviews.last_mut().unwrap();
        latest.body.push_str("\ntampered");
    }
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Err(ProviderError::InvalidResponse)
    );

    let mut wrong_tree = publication.clone();
    wrong_tree.run.trees.candidate = oid('f');
    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &wrong_tree),
        Ok(())
    );
    assert_eq!(fixture.rest.state.lock().unwrap().created.len(), 1);
}

#[test]
fn a_revoked_control_publishes_the_verdict_that_reports_it() {
    let fixture = Fixture::new("gitea");
    let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
    let publication = fixture.publication(
        snapshot,
        "evaluation-1",
        CheckConclusion::Unavailable(RunFailure::AuthorizationRevoked),
    );
    let revoked = Fixture::mutated("gitea", |data| data.protection.enable_push = true);

    assert_eq!(
        revoked.client.publish(revoked.pull_request(), &publication),
        Ok(())
    );
    let state = revoked.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 1);
    assert_eq!(state.created[0].event, ReviewState::RequestChanges);
    assert!(
        state.created[0]
            .body
            .contains("failure: authorization-revoked")
    );
}

#[test]
fn a_revoked_control_withholds_an_approval() {
    let fixture = Fixture::new("gitea");
    let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
    let publication = fixture.publication(snapshot, "evaluation-1", CheckConclusion::Pass);
    let revoked = Fixture::mutated("gitea", |data| data.protection.enable_push = true);

    assert_eq!(
        revoked.client.publish(revoked.pull_request(), &publication),
        Ok(())
    );
    assert!(revoked.rest.state.lock().unwrap().created.is_empty());
}

fn config() -> Config {
    Config {
        provider: provider("gitea"),
        reviewer: reviewer(),
        review_name: "amiss".to_owned(),
    }
}

#[test]
fn a_publication_is_validated_in_every_field() {
    let fixture = Fixture::new("gitea");
    let fresh = || {
        let snapshot = fixture.client.refresh(fixture.pull_request()).unwrap();
        fixture.publication(snapshot, "evaluation-1", CheckConclusion::Pass)
    };
    assert_eq!(
        validate_publication(&config(), fixture.pull_request(), &fresh()),
        Ok(())
    );

    let mut wrong_gate = fresh();
    wrong_gate.gate_commit = oid('9');
    let mut wrong_attempt = fresh();
    wrong_attempt.provider_run.attempt = ProviderRunAttempt::try_from(2).unwrap();
    let mut wrong_change = fresh();
    wrong_change.run.change.change =
        ChangeId::new("repository/101/pull/4201/number/43".to_owned()).unwrap();
    let mut wrong_format = fresh();
    wrong_format.run.object_format = ObjectFormat::Sha256;
    let mut wrong_forge = fresh();
    wrong_forge.run.refs.forge = ForgeDialect::Github;
    let mut wrong_candidate = fresh();
    wrong_candidate.run.commits.candidate = oid('9');
    let mut wrong_name = fresh();
    wrong_name.check.required_status_name = "other".to_owned();
    for (reason, wrong) in [
        ("gate", wrong_gate),
        ("attempt", wrong_attempt),
        ("change", wrong_change),
        ("format", wrong_format),
        ("forge", wrong_forge),
        ("candidate", wrong_candidate),
        ("status name", wrong_name),
    ] {
        assert_eq!(
            validate_publication(&config(), fixture.pull_request(), &wrong),
            Err(ProviderError::InvalidResponse),
            "{reason}"
        );
    }
}

#[test]
fn a_created_review_is_exact_fresh_and_owned() {
    let expected = CreateReview {
        event: ReviewState::Approved,
        body: "body".to_owned(),
        commit_id: oid('b'),
        comments: Vec::new(),
    };
    let review = |id: u64, user: u64, login: &str, stale: bool, dismissed: bool| ReviewRecord {
        id,
        user: Some(UserRecord {
            id: user,
            login: login.to_owned(),
            username: login.to_owned(),
            ..super::support::USER.clone()
        }),
        state: ReviewState::Approved,
        body: "body".to_owned(),
        commit_id: Some(oid('b')),
        stale,
        dismissed,
        ..super::support::REVIEW.clone()
    };

    let sound = review(9, 77, "amiss-controller", false, false);
    assert_eq!(validate_created(&config(), &expected, &sound), Ok(()));
    let loud_login = review(9, 77, "AMISS-CONTROLLER", false, false);
    assert_eq!(validate_created(&config(), &expected, &loud_login), Ok(()));

    for (reason, broken) in [
        (
            "a missing commit",
            ReviewRecord {
                commit_id: None,
                ..sound.clone()
            },
        ),
        (
            "another commit",
            ReviewRecord {
                commit_id: Some(oid('a')),
                ..sound.clone()
            },
        ),
        (
            "a different state",
            ReviewRecord {
                state: ReviewState::RequestReview,
                ..sound.clone()
            },
        ),
        (
            "an unissued id",
            review(0, 77, "amiss-controller", false, false),
        ),
        (
            "a stale review",
            review(9, 77, "amiss-controller", true, false),
        ),
        (
            "a dismissed review",
            review(9, 77, "amiss-controller", false, true),
        ),
        (
            "a foreign reviewer",
            review(9, 78, "amiss-controller", false, false),
        ),
    ] {
        assert_eq!(
            validate_created(&config(), &expected, &broken),
            Err(ProviderError::InvalidResponse),
            "{reason}"
        );
    }
}
