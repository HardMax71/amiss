use std::sync::Arc;

use amiss_controller::{
    ChangeState, CheckConclusion, ControllerError, HandleOutcome, OidPair, RunFailure, RunIdentity,
    RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat};

use crate::support::{
    FakeAdapter, complete, controller, delivery, locator, oid, provider, repository, run,
    run_with_resolution, snapshot,
};

#[test]
fn refresh_cannot_substitute_another_repository() {
    let provider = provider();
    let authenticated_change = locator(&provider, repository("amiss"));
    let wrong_change = locator(&provider, repository("other"));
    let wrong_run = run(wrong_change, 'b', 'd');
    let expected_run = run(authenticated_change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, authenticated_change, 'b'),
        [Ok(snapshot(ChangeState::Active, wrong_run))],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&expected_run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::WrongChangeIdentity)
    ));
    assert!(controller.runner.requests.is_empty());
}

#[test]
fn runner_commit_and_tree_mismatches_fail_closed() {
    let cases = [
        ('e', 'd', RunFailure::WrongIdentity),
        ('b', 'e', RunFailure::WrongTree),
    ];
    for (candidate_commit, candidate_tree, failure) in cases {
        let provider = provider();
        let change = locator(&provider, repository("amiss"));
        let expected = run(change.clone(), 'b', 'd');
        let wrong = run(change.clone(), candidate_commit, candidate_tree);
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, expected.clone())),
                Ok(snapshot(ChangeState::Active, expected.clone())),
            ],
        ));
        let mut controller = controller(Arc::clone(&adapter), complete(&wrong));

        assert_eq!(
            controller.handle(adapter.input()).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(failure))
        );
        assert_eq!(adapter.publications()[0].report, None);
    }
}

#[test]
fn runner_wrong_resolution_identity_fails_closed() {
    let cases = [
        (ForgeDialect::Github, "refs/heads/main"),
        (ForgeDialect::Gitea, "refs/heads/trunk"),
    ];
    for (forge, default_branch_ref) in cases {
        let provider = provider();
        let change = locator(&provider, repository("amiss"));
        let expected = run(change.clone(), 'b', 'd');
        let wrong = run_with_resolution(change.clone(), 'b', 'd', forge, default_branch_ref);
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, expected.clone())),
                Ok(snapshot(ChangeState::Active, expected.clone())),
            ],
        ));
        let mut controller = controller(Arc::clone(&adapter), complete(&wrong));

        assert_eq!(
            controller.handle(adapter.input()).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(RunFailure::WrongIdentity))
        );
    }
}

#[test]
fn run_identity_rejects_oids_from_another_object_format() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let invalid = RunIdentity::new(
        change,
        RunRefs {
            forge: ForgeDialect::Gitea,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        },
        ObjectFormat::Sha256,
        OidPair {
            base: oid('a'),
            candidate: oid('b'),
        },
        OidPair {
            base: oid('c'),
            candidate: oid('d'),
        },
    );

    assert!(invalid.is_none());
}
