use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use amiss_controller::{
    ChangeState, CheckConclusion, ControllerError, HandleOutcome, ProviderError, check_binding,
};

use crate::support::{
    FakeAdapter, complete, controller, delivery, locator, provider, repository, run, snapshot,
};

#[test]
fn successful_flow_binds_run_rechecks_and_publishes() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let adapter = Arc::new(FakeAdapter::new(
        authenticated.clone(),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));
    controller.runner.heartbeat_renewals = 2;

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    assert_eq!(adapter.authentication_count.load(Ordering::Relaxed), 1);
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);
    assert_eq!(controller.ledger.renewal_count, 5);
    assert_eq!(
        controller.runner.heartbeat_windows,
        vec![
            Duration::from_millis(100_002),
            Duration::from_millis(100_003),
        ]
    );
    assert_eq!(controller.runner.requests[0].run, run);
    assert_eq!(
        controller.runner.requests[0].provider_run,
        authenticated.provider_run
    );
    let request = &controller.runner.requests[0];
    assert_eq!(request.check, check_binding(&request.plan).unwrap());
    assert!(
        controller
            .plans
            .values()
            .any(|plan| Arc::ptr_eq(plan, &request.plan))
    );
    let publications = adapter.publications();
    assert_eq!(publications.len(), 1);
    assert_eq!(publications[0].conclusion, CheckConclusion::Pass);
    assert!(publications[0].report.is_some());
}

#[test]
fn completed_delivery_is_a_duplicate_without_another_run() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Ok(HandleOutcome::Published(CheckConclusion::Pass))
    ));
    assert!(matches!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Duplicate { evaluation_id }
            if evaluation_id.as_str() == "evaluation-01"
    ));
    assert_eq!(controller.runner.requests.len(), 1);
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn a_staged_publication_retries_without_another_run() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(
        FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        )
        .with_publish_results([Err(ProviderError::Unavailable), Ok(())]),
    );
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::Publish(ProviderError::Unavailable))
    ));
    let first = adapter.publications();
    assert_eq!(first.len(), 1);

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    let retried = adapter.publications();
    assert_eq!(retried.len(), 2);
    assert_eq!(retried.first(), retried.get(1));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);

    assert!(matches!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Duplicate { .. }
    ));
    assert_eq!(adapter.publications().len(), 2);
}

#[test]
fn incomplete_claim_resumes_after_a_transient_refresh_failure() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Err(ProviderError::Unavailable),
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::Provider(_))
    ));
    assert!(matches!(
        controller.handle(adapter.input()),
        Ok(HandleOutcome::Published(CheckConclusion::Pass))
    ));
    assert_eq!(
        controller.runner.requests[0].evaluation_id.as_str(),
        "evaluation-01"
    );
}

#[test]
fn resumed_delivery_cannot_follow_the_changes_new_head() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let original = run(change.clone(), 'b', 'd');
    let moved = run(change.clone(), 'e', 'f');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Err(ProviderError::Unavailable),
            Ok(snapshot(ChangeState::Active, moved)),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&original));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::Provider(_))
    ));
    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::WrongProviderRun)
    ));
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}
