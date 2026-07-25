use amiss_controller::{CheckConclusion, ProviderError, RunFailure};

use super::support::{Fixture, oid};

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
