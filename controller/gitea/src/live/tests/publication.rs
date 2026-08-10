use amiss_controller::{ChangeId, CheckConclusion, ProviderError, ProviderRunAttempt, RunFailure};
use amiss_wire::model::{ForgeDialect, ObjectFormat};

use super::super::Config;
use super::super::model::{CreateReview, ReviewRecord, UserRecord};
use super::super::publication::{validate_created, validate_publication};
use super::support::{Fixture, oid, provider, reviewer};

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
    assert_eq!(state.created[0].event, "APPROVED");
    assert_eq!(state.created[0].commit_id, oid('b').as_str());
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
    assert_eq!(state.created[1].event, "REQUEST_CHANGES");
    drop(state);

    assert_eq!(
        fixture.client.publish(fixture.pull_request(), &publication),
        Ok(())
    );
    let state = fixture.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 2);
    assert_eq!(state.created[1].event, "REQUEST_CHANGES");
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
    let revoked = Fixture::mutated("gitea", |data| data.protection.writes.enable_push = true);

    assert_eq!(
        revoked.client.publish(revoked.pull_request(), &publication),
        Ok(())
    );
    let state = revoked.rest.state.lock().unwrap();
    assert_eq!(state.created.len(), 1);
    assert_eq!(state.created[0].event, "REQUEST_CHANGES");
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
    let revoked = Fixture::mutated("gitea", |data| data.protection.writes.enable_push = true);

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
    wrong_attempt.provider_run.attempt = ProviderRunAttempt::new(2).unwrap();
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
        event: "APPROVED".to_owned(),
        body: "body".to_owned(),
        commit_id: oid('b').as_str().to_owned(),
        comments: Vec::new(),
    };
    let review = |id: u64, user: u64, login: &str, stale: bool, dismissed: bool| ReviewRecord {
        id,
        user: Some(UserRecord {
            id: user,
            login: login.to_owned(),
        }),
        state: "APPROVED".to_owned(),
        body: "body".to_owned(),
        commit_id: oid('b').as_str().to_owned(),
        stale,
        dismissed,
    };

    let sound = review(9, 77, "amiss-controller", false, false);
    assert_eq!(validate_created(&config(), &expected, &sound), Ok(()));
    let loud_login = review(9, 77, "AMISS-CONTROLLER", false, false);
    assert_eq!(validate_created(&config(), &expected, &loud_login), Ok(()));

    for (reason, broken) in [
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
