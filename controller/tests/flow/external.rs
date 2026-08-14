use std::sync::Arc;
use std::sync::atomic::Ordering;

use amiss_controller::{
    ChangeState, CheckConclusion, Evaluation, ExternalTally, HandleOutcome, ProviderError,
    RunnerOutcome,
};
use amiss_wire::external::{evidence_file, forge_evidence_row};
use amiss_wire::json::Value;

use crate::support::{
    FakeAdapter, RecordingSink, controller, delivery, locator, provider, repository, run, snapshot,
};

const DESTINATION: &str = "https://github.com/acme/widgets/blob/main/a.md";

fn external_outcome(run: &amiss_controller::RunIdentity) -> RunnerOutcome {
    RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: Evaluation::Pass,
        report: amiss_fixtures::external_report(&[DESTINATION]),
    }
}

/// The evidence a scripted provider answers with: bound to the exact plan
/// the controller will derive from the same report bytes.
fn scripted_evidence() -> Value {
    let report = amiss_fixtures::external_report(&[DESTINATION]);
    let parsed = amiss_wire::json::parse(&report).unwrap();
    let engine = parsed
        .member("payload")
        .and_then(|payload| payload.member("engine"))
        .unwrap();
    let plan = amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version").unwrap(),
        engine.text("engine_digest").unwrap(),
    )
    .unwrap();
    let row = forge_evidence_row(DESTINATION, "readable", Some("path-missing"), "t0");
    evidence_file(&plan, "scripted", "0", vec![row]).unwrap()
}

fn published_with(
    verify: impl IntoIterator<Item = Result<Option<Value>, ProviderError>>,
    sink: Option<&Arc<RecordingSink>>,
) -> Arc<FakeAdapter> {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let adapter = Arc::new(
        FakeAdapter::new(
            authenticated,
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        )
        .with_verify_results(verify),
    );
    let mut controller = controller(Arc::clone(&adapter), external_outcome(&run));
    if let Some(sink) = sink {
        let sink = Arc::clone(sink);
        controller = controller.with_external_sink(sink);
    }
    assert_eq!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    adapter
}

#[test]
fn a_published_delivery_is_advisorily_assessed() {
    let sink = Arc::new(RecordingSink::default());
    let adapter = published_with([Ok(Some(scripted_evidence()))], Some(&sink));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
    assert_eq!(
        sink.tallies.lock().unwrap().clone(),
        vec![ExternalTally {
            refuted: 1,
            unproven: 0,
            reachable: 0,
        }],
    );
    assert_eq!(sink.incomplete.load(Ordering::Relaxed), 0);
}

/// The verdict is sealed before verification starts; a failing verifier is
/// one incomplete tick and nothing else.
#[test]
fn a_failing_verifier_never_touches_the_verdict() {
    let sink = Arc::new(RecordingSink::default());
    let adapter = published_with([Err(ProviderError::Unavailable)], Some(&sink));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
    assert!(sink.tallies.lock().unwrap().is_empty());
    assert_eq!(sink.incomplete.load(Ordering::Relaxed), 1);
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn without_a_sink_no_verification_runs() {
    let adapter = published_with([Ok(Some(scripted_evidence()))], None);
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 0);
}
