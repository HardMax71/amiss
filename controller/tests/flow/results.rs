use std::sync::Arc;

use amiss_controller::{
    ChangeState, CheckConclusion, Evaluation, HandleOutcome, RunFailure, RunnerOutcome,
};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::support::{
    FakeAdapter, complete, controller, delivery, locator, provider, repository, run, snapshot,
};

#[test]
fn provider_supersession_is_published_for_the_original_candidate() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let initial = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, initial.clone())),
            Ok(snapshot(ChangeState::Superseded, initial.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&initial));

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Superseded)
    );
}

#[test]
fn revoked_authorization_overrides_a_successful_runner() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::AuthorizationRevoked, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Unavailable(
            RunFailure::AuthorizationRevoked
        ))
    );
}

#[test]
fn missing_timeout_and_tampered_results_all_fail_closed() {
    let cases = [
        (RunnerOutcome::MissingOutput, RunFailure::MissingOutput),
        (RunnerOutcome::OversizedOutput, RunFailure::OversizedOutput),
        (RunnerOutcome::TimedOut, RunFailure::Timeout),
        (RunnerOutcome::TamperedRuntime, RunFailure::TamperedRuntime),
    ];
    for (outcome, failure) in cases {
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
        let mut controller = controller(Arc::clone(&adapter), outcome);

        assert_eq!(
            controller.handle(adapter.input()).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(failure))
        );
    }
}

#[test]
fn oversized_report_is_not_accepted_for_publication() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let oversized = usize::try_from(MACHINE_JSON_BYTES).unwrap() + 1;
    let outcome = RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: Evaluation::Pass,
        report: vec![b'x'; oversized],
    };
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run)),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), outcome);

    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Unavailable(RunFailure::OversizedOutput))
    );
}
