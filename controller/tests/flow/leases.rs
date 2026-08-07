use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use amiss_controller::{
    AuthenticatedDelivery, ChangeState, CheckConclusion, ControllerError, ControllerEvaluationId,
    DeliveryClaim, DeliveryLease, LeaseCompletion, LeaseFence, LeaseRenewal, Publication,
    RunIdentity, StageOutcome, StagedPublication,
};
use amiss_wire::model::{ObjectFormat, Oid};

use crate::support::{
    FakeAdapter, LedgerError, ScriptedLedger, binding, complete, controller_with_ledger, delivery,
    lease, locator, provider, renewal_script, repository, run, snapshot,
};

#[test]
fn renewal_failure_during_or_after_a_run_stops_before_publication() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expected_lease = lease();
    let changed_fence = DeliveryLease {
        fence: LeaseFence::new(2).unwrap(),
        ..expected_lease.clone()
    };
    for (heartbeat_renewals, failure) in [
        (0, Ok(LeaseRenewal::Lost)),
        (1, Ok(LeaseRenewal::Lost)),
        (1, Err(LedgerError)),
        (1, Ok(LeaseRenewal::Renewed(changed_fence))),
    ] {
        let expect_ledger_error = failure.is_err();
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change.clone(), 'b'),
            [Ok(snapshot(ChangeState::Active, run.clone()))],
        ));
        let ledger = ScriptedLedger {
            claim: Some(DeliveryClaim::Execute(expected_lease.clone())),
            renewals: VecDeque::from([Ok(LeaseRenewal::Renewed(expected_lease.clone())), failure]),
            stage: None,
            completion: LeaseCompletion::Lost,
        };
        let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
        controller.runner.heartbeat_renewals = heartbeat_renewals;

        let result = controller.handle(adapter.input());
        if expect_ledger_error {
            assert!(matches!(result, Err(ControllerError::Ledger(LedgerError))));
        } else {
            assert!(matches!(result, Err(ControllerError::LeaseLost)));
        }
        assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 1);
        assert_eq!(controller.runner.requests.len(), 1);
        assert!(adapter.publications().is_empty());
    }
}

#[test]
fn a_renewed_lease_without_time_left_stops_the_run() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expired = DeliveryLease {
        expires_at_unix_millis: 1_800_000_000_000,
        ..lease()
    };
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [Ok(snapshot(ChangeState::Active, run.clone()))],
    ));
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Execute(expired.clone())),
        renewals: renewal_script([
            LeaseRenewal::Renewed(expired.clone()),
            LeaseRenewal::Renewed(expired),
        ]),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    controller.runner.heartbeat_renewals = 1;

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));
    assert_eq!(controller.runner.requests.len(), 1);
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_publication_must_be_staged_under_the_live_fence() {
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
    let expected = lease();
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Execute(expected.clone())),
        renewals: renewal_script([
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected),
        ]),
        stage: Some(StageOutcome::Lost),
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_lost_completion_record_is_distinct_after_publication() {
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
    let expected = lease();
    let staged = StagedPublication {
        evaluation_id: expected.evaluation_id.clone(),
        fence: expected.fence,
        publication: Box::new(Publication {
            provider_run: authenticated.provider_run,
            evaluation_id: expected.evaluation_id.clone(),
            check: expected.check.clone(),
            run: run.clone(),
            gate_commit: run.commits.candidate.clone(),
            conclusion: CheckConclusion::Pass,
            report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
        }),
    };
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Execute(expected.clone())),
        renewals: renewal_script([
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected),
        ]),
        stage: Some(StageOutcome::Staged(staged)),
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::CompletionLost)
    ));
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn a_staged_gate_must_use_the_publication_runs_object_format() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let expected = lease();
    let staged = StagedPublication {
        evaluation_id: expected.evaluation_id.clone(),
        fence: expected.fence,
        publication: Box::new(Publication {
            provider_run: authenticated.provider_run.clone(),
            evaluation_id: expected.evaluation_id.clone(),
            check: expected.check.clone(),
            run: run.clone(),
            gate_commit: Oid::new(ObjectFormat::Sha256, "e".repeat(64)).unwrap(),
            conclusion: CheckConclusion::Pass,
            report: None,
        }),
    };
    let adapter = Arc::new(FakeAdapter::new(authenticated, []));
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Publish(staged)),
        renewals: VecDeque::new(),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::WrongProviderRun)
    ));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 0);
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_ledger_cannot_change_the_lease_during_renewal() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expected = lease();
    let changed_evaluation = DeliveryLease {
        evaluation_id: ControllerEvaluationId::new("evaluation-02".to_owned()).unwrap(),
        ..expected.clone()
    };
    let changed_fence = DeliveryLease {
        fence: LeaseFence::new(2).unwrap(),
        ..expected.clone()
    };
    let shortened = DeliveryLease {
        expires_at_unix_millis: expected.expires_at_unix_millis - 1,
        ..expected.clone()
    };
    let mut other_check = expected.check.clone();
    other_check.required_status_name = "amiss / another check".to_owned();
    let changed_check = DeliveryLease {
        check: other_check,
        ..expected.clone()
    };

    for changed in [changed_evaluation, changed_fence, shortened, changed_check] {
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change.clone(), 'b'),
            [Ok(snapshot(ChangeState::Active, run.clone()))],
        ));
        let ledger = ScriptedLedger {
            claim: Some(DeliveryClaim::Execute(expected.clone())),
            renewals: renewal_script([LeaseRenewal::Renewed(changed)]),
            stage: None,
            completion: LeaseCompletion::Lost,
        };
        let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

        assert!(matches!(
            controller.handle(adapter.input()),
            Err(ControllerError::LeaseLost)
        ));
        assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 1);
        assert!(controller.runner.requests.is_empty());
        assert!(adapter.publications().is_empty());
    }
}

fn submitted_publication(run: &RunIdentity, delivery: &AuthenticatedDelivery) -> Publication {
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: ControllerEvaluationId::new("evaluation-01".to_owned()).unwrap(),
        check: binding(),
        run: run.clone(),
        gate_commit: run.commits.candidate.clone(),
        conclusion: CheckConclusion::Pass,
        report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
    }
}

/// A staged row must echo the lease and the submitted publication exactly:
/// the aligned row completes, and one drifted field at a time loses the
/// lease, whether the drift sits in the evaluation, the publication body, or
/// the claim's check binding.
#[test]
fn a_staged_row_must_echo_the_lease_and_publication_exactly() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expected = lease();
    let scripted = |staged: Option<StagedPublication>, claim: DeliveryLease| {
        (
            Arc::new(FakeAdapter::new(
                delivery(&provider, change.clone(), 'b'),
                [
                    Ok(snapshot(ChangeState::Active, run.clone())),
                    Ok(snapshot(ChangeState::Active, run.clone())),
                ],
            )),
            ScriptedLedger {
                claim: Some(DeliveryClaim::Execute(claim)),
                renewals: renewal_script([
                    LeaseRenewal::Renewed(expected.clone()),
                    LeaseRenewal::Renewed(expected.clone()),
                    LeaseRenewal::Renewed(expected.clone()),
                ]),
                stage: staged.map(StageOutcome::Staged),
                completion: LeaseCompletion::Completed,
            },
        )
    };
    let aligned = |publication: Publication| StagedPublication {
        evaluation_id: expected.evaluation_id.clone(),
        fence: expected.fence,
        publication: Box::new(publication),
    };

    let staged_publication = {
        let delivery = delivery(&provider, change.clone(), 'b');
        aligned(submitted_publication(&run, &delivery))
    };
    let (adapter, ledger) = scripted(Some(staged_publication.clone()), expected.clone());
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    assert!(
        controller.handle(adapter.input()).is_ok(),
        "the aligned staging completes"
    );

    let mut wrong_evaluation = staged_publication.clone();
    wrong_evaluation.evaluation_id =
        ControllerEvaluationId::new("evaluation-02".to_owned()).unwrap();
    let (adapter, ledger) = scripted(Some(wrong_evaluation), expected.clone());
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));

    let mut wrong_fence = staged_publication.clone();
    wrong_fence.fence = LeaseFence::new(3).unwrap();
    let (adapter, ledger) = scripted(Some(wrong_fence), expected.clone());
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));

    let mut wrong_publication = staged_publication.clone();
    wrong_publication.publication.report = Some(b"{}".to_vec());
    let (adapter, ledger) = scripted(Some(wrong_publication), expected.clone());
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));

    let mut drifted_check = expected.clone();
    drifted_check.check.required_status_name = "amiss / elsewhere".to_owned();
    let (adapter, mut ledger) = scripted(None, drifted_check.clone());
    ledger.renewals = renewal_script([
        LeaseRenewal::Renewed(drifted_check.clone()),
        LeaseRenewal::Renewed(drifted_check.clone()),
        LeaseRenewal::Renewed(drifted_check),
    ]);
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
    assert!(matches!(
        controller.handle(adapter.input()),
        Err(ControllerError::LeaseLost)
    ));
}
