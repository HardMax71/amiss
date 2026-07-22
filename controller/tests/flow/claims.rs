use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use amiss_controller::{
    ControllerError, DeliveryClaim, HandleOutcome, IngressError, LeaseCompletion, ProviderInstance,
};

use crate::support::{
    FakeAdapter, ScriptedLedger, complete, controller, controller_with_ledger, delivery, lease,
    locator, provider, repository, run,
};

#[test]
fn an_oversized_raw_body_stops_before_authentication_or_claim() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&provider, change, 'b'), []));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));
    let body = vec![b'x'; 1_025];

    assert!(matches!(
        controller.handle(adapter.input_with_body(&body)),
        Err(ControllerError::Ingress(IngressError::Limits))
    ));
    assert_eq!(adapter.authentication_count.load(Ordering::Relaxed), 0);
    assert!(controller.ledger.is_empty());
}

#[test]
fn a_live_lease_is_an_expected_in_progress_outcome() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&provider, change, 'b'), []));
    let expected_lease = lease();
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Busy {
            evaluation_id: expected_lease.evaluation_id.clone(),
            retry_at_unix_millis: expected_lease.expires_at_unix_millis,
        }),
        renewals: VecDeque::new(),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::InProgress {
            evaluation_id: expected_lease.evaluation_id,
            retry_at_unix_millis: expected_lease.expires_at_unix_millis,
        }
    );
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 0);
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_conflicting_delivery_binding_fails_before_refresh() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&provider, change, 'b'), []));
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::BindingConflict),
        renewals: VecDeque::new(),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::DeliveryBindingConflict)
    ));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 0);
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn authenticated_provider_must_match_the_routed_instance() {
    let actual = provider();
    let mut expected = actual.clone();
    expected.instance = ProviderInstance::new("other.example.test".to_owned()).unwrap();
    let change = locator(&actual, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(
        FakeAdapter::new(delivery(&actual, change, 'b'), []).with_route_provider(expected),
    );
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::Ingress(IngressError::Route))
    ));
    assert!(controller.ledger.is_empty());
    assert!(controller.runner.requests.is_empty());
}
