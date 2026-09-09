use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use amiss_controller::{
    ArtifactComponent, ArtifactStoreConfig, ChangeState, CheckConclusion, Controller, Evaluation,
    ExternalPolicy, ExternalTally, FileArtifactStore, FileLedger, FileLedgerConfig, HandleOutcome,
    ProviderError, ReplayWindow, RunnerOutcome, check_plan,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_wire::external::{
    ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow, ExternalEvidenceSchema,
    ForgeRepository, ForgeTail,
};

use crate::support::{
    FakeAdapter, RecordingSink, controller, controller_with_ledger, delivery, locator, oid,
    provider, repository, run, snapshot,
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
        report: amiss_fixtures::captured_report(
            serde_json_canonicalizer::to_vec(
                &amiss_fixtures::external_report(&[DESTINATION]).unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
        semantic_artifact: None,
    }
}

fn scripted_evidence(repository: ForgeRepository, tail: Option<ForgeTail>) -> ExternalEvidence {
    let plan = amiss_fixtures::external_plan(&[DESTINATION]).unwrap();
    ExternalEvidence {
        schema: ExternalEvidenceSchema::Current,
        plan_payload_digest: plan.payload_digest,
        producer: ExternalEvidenceProducer {
            name: "scripted".to_owned(),
            version: "0".to_owned(),
        },
        rows: vec![ExternalEvidenceRow::ForgeApi {
            destination: DESTINATION.to_owned(),
            repository,
            tail,
            checked_at: "t0".to_owned(),
        }],
    }
}

fn set_external_policy<L, R>(controller: &mut Controller<L, R>, external_policy: ExternalPolicy) {
    let mut current = controller.plans.values().next().unwrap().as_ref().clone();
    current.policy.external_policy = external_policy;
    let changed = check_plan(current.profile, current.policy, current.execution).unwrap();
    *controller.plans.values_mut().next().unwrap() = Arc::new(changed);
}

fn published_with(
    external_policy: ExternalPolicy,
    verify: impl IntoIterator<Item = Result<Option<ExternalEvidence>, ProviderError>>,
    sink: Option<&Arc<RecordingSink>>,
) -> (Arc<FakeAdapter>, HandleOutcome) {
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
    set_external_policy(&mut controller, external_policy);
    let root = tempfile::tempdir().unwrap();
    let artifacts = Arc::new(
        FileArtifactStore::open_with_clock(root.path(), artifact_config(), TestClock::new())
            .unwrap(),
    );
    controller = controller.with_artifact_store(artifacts);
    if let Some(sink) = sink {
        let sink = Arc::clone(sink);
        controller = controller.with_external_sink(sink);
    }
    let outcome = controller.handle(adapter.input()).unwrap();
    (adapter, outcome)
}

#[test]
fn a_published_delivery_is_advisorily_assessed() {
    let sink = Arc::new(RecordingSink::default());
    let (adapter, outcome) = published_with(
        ExternalPolicy::Advisory,
        [Ok(Some(scripted_evidence(
            ForgeRepository::Readable,
            Some(ForgeTail::PathMissing),
        )))],
        Some(&sink),
    );
    assert!(matches!(
        outcome,
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(_),
        }
    ));
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

#[test]
fn an_incomplete_verification_never_blocks() {
    let mut invalid = scripted_evidence(ForgeRepository::Readable, Some(ForgeTail::PathMissing));
    invalid.producer.version.clear();
    let mut foreign = scripted_evidence(ForgeRepository::Readable, Some(ForgeTail::PathMissing));
    foreign.plan_payload_digest = amiss_wire::digest::hb("foreign", b"plan");
    let mut oversized = scripted_evidence(ForgeRepository::Readable, Some(ForgeTail::PathMissing));
    oversized.producer.version =
        "\0".repeat(usize::try_from(amiss_wire::external::EXTERNAL_DOCUMENT_BYTES / 6).unwrap());
    for (result, incomplete) in [
        (Err(ProviderError::Unavailable), true),
        (Ok(None), false),
        (Ok(Some(invalid)), true),
        (Ok(Some(foreign)), true),
        (Ok(Some(oversized)), true),
    ] {
        let sink = Arc::new(RecordingSink::default());
        let (adapter, outcome) = published_with(
            ExternalPolicy::BlockConfirmedRefutations,
            [result],
            Some(&sink),
        );
        let HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(reference),
        } = outcome
        else {
            panic!("incomplete evidence must preserve the report and plan without blocking");
        };
        assert_eq!(reference.external_incomplete, incomplete);
        assert!(reference.external_tally.is_none());
        assert!(reference.assessment_digest.is_none());
        assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
        assert!(sink.tallies.lock().unwrap().is_empty());
        assert_eq!(
            sink.incomplete.load(Ordering::Relaxed),
            usize::from(incomplete)
        );
        assert_eq!(adapter.publications().len(), 1);
    }
}

#[test]
fn an_off_policy_skips_verification() {
    let sink = Arc::new(RecordingSink::default());
    let (adapter, outcome) = published_with(
        ExternalPolicy::Off,
        [Ok(Some(scripted_evidence(
            ForgeRepository::Readable,
            Some(ForgeTail::PathMissing),
        )))],
        Some(&sink),
    );
    assert!(matches!(
        outcome,
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(_),
        }
    ));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 0);
    assert!(sink.tallies.lock().unwrap().is_empty());
    assert_eq!(sink.incomplete.load(Ordering::Relaxed), 0);
}

#[test]
fn only_a_confirmed_refutation_blocks() {
    let (refuted, refuted_outcome) = published_with(
        ExternalPolicy::BlockConfirmedRefutations,
        [Ok(Some(scripted_evidence(
            ForgeRepository::Readable,
            Some(ForgeTail::PathMissing),
        )))],
        None,
    );
    assert!(matches!(
        refuted_outcome,
        HandleOutcome::Published {
            conclusion: CheckConclusion::Block,
            artifact: Some(_),
        }
    ));
    assert_eq!(refuted.verify_count.load(Ordering::Relaxed), 1);

    let (unproven, unproven_outcome) = published_with(
        ExternalPolicy::BlockConfirmedRefutations,
        [Ok(Some(scripted_evidence(ForgeRepository::Missing, None)))],
        None,
    );
    assert!(matches!(
        unproven_outcome,
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            artifact: Some(_),
        }
    ));
    assert_eq!(unproven.verify_count.load(Ordering::Relaxed), 1);
}

#[test]
fn the_final_refresh_supersedes_a_retained_external_decision() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let initial = snapshot(ChangeState::Active, run.clone());
    let mut moved_gate = initial.clone();
    moved_gate.gate_commit = oid('e');
    let adapter = Arc::new(
        FakeAdapter::new(authenticated, [Ok(initial), Ok(moved_gate)]).with_verify_results([Ok(
            Some(scripted_evidence(
                ForgeRepository::Readable,
                Some(ForgeTail::PathMissing),
            )),
        )]),
    );
    let mut controller = controller(Arc::clone(&adapter), external_outcome(&run));
    set_external_policy(&mut controller, ExternalPolicy::BlockConfirmedRefutations);
    let root = tempfile::tempdir().unwrap();
    controller = controller.with_artifact_store(Arc::new(
        FileArtifactStore::open_with_clock(root.path(), artifact_config(), TestClock::new())
            .unwrap(),
    ));

    assert!(matches!(
        controller.handle(adapter.input()).unwrap(),
        HandleOutcome::Published {
            conclusion: CheckConclusion::Superseded,
            artifact: Some(_),
        }
    ));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
}

#[test]
fn a_lost_reply_and_service_restart_reuse_the_frozen_artifact() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let evidence_input = scripted_evidence(ForgeRepository::Readable, Some(ForgeTail::PathMissing));
    let adapter = Arc::new(
        FakeAdapter::new(
            authenticated.clone(),
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        )
        .with_verify_results([Ok(Some(evidence_input.clone()))])
        .with_publish_results([Err(ProviderError::Unavailable)]),
    );
    let ledger_root = tempfile::tempdir().unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_secs(30)).unwrap();
    let ledger_config = FileLedgerConfig::new(Duration::from_mins(1), 8, replay).unwrap();
    let artifact_config = artifact_config();
    let ledger =
        FileLedger::open_with_clock(ledger_root.path(), ledger_config, TestClock::new()).unwrap();
    let artifacts = Arc::new(
        FileArtifactStore::open_with_clock(
            artifact_root.path(),
            artifact_config.clone(),
            TestClock::new(),
        )
        .unwrap(),
    );
    let mut controller =
        controller_with_ledger(Arc::clone(&adapter), ledger, external_outcome(&run))
            .with_artifact_store(Arc::clone(&artifacts));
    set_external_policy(&mut controller, ExternalPolicy::BlockConfirmedRefutations);

    assert!(matches!(
        controller.handle(adapter.input()),
        Err(amiss_controller::ControllerError::Publish(
            ProviderError::Unavailable
        ))
    ));
    let first = adapter.publications().remove(0);
    let retained = first.artifact.clone().unwrap();
    let assessment = artifacts
        .read(&retained.id, ArtifactComponent::Assessment)
        .unwrap();
    let evidence = artifacts
        .read(&retained.id, ArtifactComponent::Evidence)
        .unwrap();
    let (parsed_evidence, digest) = amiss_wire::external::parse_evidence(&evidence).unwrap();
    assert_eq!(parsed_evidence, evidence_input);
    let parsed_assessment = amiss_wire::external::parse_assessment(&assessment).unwrap();
    assert_eq!(parsed_assessment.payload.subject.evidence_digest, digest);
    drop(controller);
    drop(artifacts);

    let retry_adapter = Arc::new(FakeAdapter::new(authenticated, []));
    let reopened_ledger =
        FileLedger::open_with_clock(ledger_root.path(), ledger_config, TestClock::new()).unwrap();
    let reopened_artifacts = Arc::new(
        FileArtifactStore::open_with_clock(artifact_root.path(), artifact_config, TestClock::new())
            .unwrap(),
    );
    let mut restarted = controller_with_ledger(
        Arc::clone(&retry_adapter),
        reopened_ledger,
        external_outcome(&run),
    )
    .with_artifact_store(Arc::clone(&reopened_artifacts));
    set_external_policy(&mut restarted, ExternalPolicy::BlockConfirmedRefutations);

    assert!(matches!(
        restarted.handle(retry_adapter.input()).unwrap(),
        HandleOutcome::Published {
            conclusion: CheckConclusion::Block,
            artifact: Some(reference),
        } if reference == retained
    ));
    assert_eq!(adapter.verify_count.load(Ordering::Relaxed), 1);
    assert_eq!(retry_adapter.verify_count.load(Ordering::Relaxed), 0);
    assert_eq!(retry_adapter.publications(), vec![first]);
    for (component, bytes) in [
        (ArtifactComponent::Evidence, evidence),
        (ArtifactComponent::Assessment, assessment),
    ] {
        assert_eq!(
            reopened_artifacts.read(&retained.id, component).unwrap(),
            bytes
        );
    }
    assert!(matches!(
        restarted.handle(retry_adapter.input()).unwrap(),
        HandleOutcome::Duplicate {
            artifact: Some(reference),
            ..
        } if reference == retained
    ));
    assert_eq!(retry_adapter.verify_count.load(Ordering::Relaxed), 0);
}
