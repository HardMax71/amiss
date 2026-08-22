use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use amiss_controller::{
    ArtifactStoreConfig, ChangeState, CheckConclusion, Evaluation, ExternalTally,
    FileArtifactStore, FileLedger, FileLedgerConfig, HandleOutcome, ProviderError, ReplayWindow,
    RunnerOutcome, SystemClock,
};
use amiss_wire::external::{evidence_file, forge_evidence_row};
use amiss_wire::json::Value;

use crate::support::{
    FakeAdapter, RecordingSink, controller, controller_with_ledger, delivery, locator, provider,
    repository, run, snapshot,
};

const DESTINATION: &str = "https://github.com/acme/widgets/blob/main/a.md";

fn artifact_config() -> ArtifactStoreConfig {
    ArtifactStoreConfig {
        base_url: "https://amiss.example/artifacts".to_owned(),
        retention: Duration::from_hours(1),
        max_records: 8,
        max_bytes: 1_048_576,
        max_record_bytes: 1_048_576,
    }
}

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
    let root = tempfile::tempdir().unwrap();
    let artifacts = Arc::new(
        FileArtifactStore::open_with_clock(root.path(), artifact_config(), Arc::new(SystemClock))
            .unwrap(),
    );
    controller = controller.with_artifact_store(artifacts);
    if let Some(sink) = sink {
        let sink = Arc::clone(sink);
        controller = controller.with_external_sink(sink);
    }
    assert!(matches!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(_),
        }
    ));
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

#[test]
fn a_lost_reply_and_service_restart_reuse_the_frozen_artifact() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let adapter = Arc::new(
        FakeAdapter::new(
            authenticated.clone(),
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        )
        .with_verify_results([Ok(Some(scripted_evidence()))])
        .with_publish_results([Err(ProviderError::Unavailable)]),
    );
    let ledger_root = tempfile::tempdir().unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_secs(30)).unwrap();
    let ledger_config = FileLedgerConfig::new(Duration::from_mins(1), 8, replay).unwrap();
    let artifact_config = artifact_config();
    let ledger =
        FileLedger::open_with_clock(ledger_root.path(), ledger_config, Arc::new(SystemClock))
            .unwrap();
    let artifacts = Arc::new(
        FileArtifactStore::open_with_clock(
            artifact_root.path(),
            artifact_config.clone(),
            Arc::new(SystemClock),
        )
        .unwrap(),
    );
    let sink = Arc::new(RecordingSink::default());
    let mut controller =
        controller_with_ledger(Arc::clone(&adapter), ledger, external_outcome(&run))
            .with_external_sink(sink)
            .with_artifact_store(Arc::clone(&artifacts));

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(amiss_controller::ControllerError::Publish(
            ProviderError::Unavailable
        ))
    ));
    let first = adapter.publications().remove(0);
    let retained = first.artifact.clone().unwrap();
    let assessment = artifacts
        .read(
            &retained.id,
            amiss_controller::ArtifactComponent::Assessment,
        )
        .unwrap();
    drop(controller);
    drop(artifacts);

    let retry_adapter = Arc::new(FakeAdapter::new(authenticated, []));
    let reopened_ledger =
        FileLedger::open_with_clock(ledger_root.path(), ledger_config, Arc::new(SystemClock))
            .unwrap();
    let reopened_artifacts = Arc::new(
        FileArtifactStore::open_with_clock(
            artifact_root.path(),
            artifact_config,
            Arc::new(SystemClock),
        )
        .unwrap(),
    );
    let retry_sink = Arc::new(RecordingSink::default());
    let mut restarted = controller_with_ledger(
        Arc::clone(&retry_adapter),
        reopened_ledger,
        external_outcome(&run),
    )
    .with_external_sink(retry_sink)
    .with_artifact_store(Arc::clone(&reopened_artifacts));

    assert!(matches!(
        restarted.handle(retry_adapter.input()).unwrap(),
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(reference),
        } if reference == retained
    ));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
    assert_eq!(retry_adapter.verify_count.load(Ordering::Relaxed), 0);
    assert_eq!(retry_adapter.publications(), vec![first]);
    assert_eq!(
        reopened_artifacts
            .read(
                &retained.id,
                amiss_controller::ArtifactComponent::Assessment
            )
            .unwrap(),
        assessment
    );
    assert!(matches!(
        restarted.handle(retry_adapter.input()).unwrap(),
        HandleOutcome::Duplicate {
            artifact: Some(reference),
            ..
        } if reference == retained
    ));
    assert_eq!(retry_adapter.verify_count.load(Ordering::Relaxed), 0);
}
