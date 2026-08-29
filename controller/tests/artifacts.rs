use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactBundle, ArtifactComponent, ArtifactError, ArtifactStoreConfig, ControllerClock,
    ControllerEvaluationId, ExternalTally, FileArtifactStore, SemanticEvidenceTemplate,
    bind_semantic_evidence,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_wire::digest::{hb, sha256};
use amiss_wire::json;
use amiss_wire::model::ArtifactId;

fn config() -> ArtifactStoreConfig {
    ArtifactStoreConfig {
        base_url: "https://amiss.example/artifacts".to_owned(),
        retention: Duration::from_secs(1),
        max_records: 4,
        max_bytes: 1_048_576,
        max_record_bytes: 524_288,
    }
}

#[test]
fn exact_components_survive_restart_under_one_stable_locator() {
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let evaluation = ControllerEvaluationId::new("evaluation/1".to_owned()).unwrap();
    let report = br#"{"payload":{"feedback":{"items":[],"status":"available"}}}"#;
    let plan = br#"{"schema":"amiss/external-plan-envelope"}"#;
    let evidence = br#"{"schema":"amiss/external-evidence"}"#;
    let assessment = br#"{"schema":"amiss/external-assessment-envelope"}"#;
    let bundle = ArtifactBundle {
        report,
        semantic: None,
        plan: Some(plan),
        evidence: Some(evidence),
        assessment: Some(assessment),
        external_tally: Some(ExternalTally {
            refuted: 1,
            unproven: 2,
            reachable: 3,
        }),
        external_incomplete: false,
    };
    let retained = store.retain(&evaluation, bundle).unwrap();

    assert_eq!(store.retain(&evaluation, bundle).unwrap(), retained);
    assert_eq!(
        store.read(&retained.id, ArtifactComponent::Report).unwrap(),
        report
    );
    assert_eq!(
        store.read(&retained.id, ArtifactComponent::Plan).unwrap(),
        plan
    );
    assert_eq!(
        store
            .read(&retained.id, ArtifactComponent::Evidence)
            .unwrap(),
        evidence
    );
    assert_eq!(
        store
            .read(&retained.id, ArtifactComponent::Assessment)
            .unwrap(),
        assessment
    );
    assert_eq!(store.find(&evaluation).unwrap(), Some(retained.clone()));
    drop(store);

    let reopened =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    reopened.verify(&retained).unwrap();
    assert_eq!(reopened.find(&evaluation).unwrap(), Some(retained));
}

#[test]
fn semantic_inputs_survive_restart_under_the_report_binding() {
    let bound = bind_semantic_evidence(
        &[SemanticEvidenceTemplate {
            producer_kind: ArtifactId::new("record-set".to_owned()).unwrap(),
            producer_identity: ArtifactId::new("test-records".to_owned()).unwrap(),
            producer_version: "1".to_owned(),
            context_digest: hb("amiss/test-context", b"context"),
            input_digest: hb("amiss/test-input", b"input"),
            complete: true,
            observations: Arc::from([]),
        }],
        &[],
        &[],
        hb("amiss/test-candidate", b"candidate"),
    )
    .unwrap();
    let payload_digests = bound
        .supplied
        .iter()
        .map(|supplied| {
            amiss_wire::semantic::parse(&json::canonical(&supplied.value))
                .unwrap()
                .payload_digest
        })
        .collect::<Vec<_>>();
    let report = amiss_fixtures::semantic_report(&payload_digests);
    let semantic = bound.artifact.unwrap();
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let evaluation = ControllerEvaluationId::new("evaluation/semantic".to_owned()).unwrap();
    let reference = store
        .retain(
            &evaluation,
            ArtifactBundle {
                report: &report,
                semantic: Some(&semantic),
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            },
        )
        .unwrap();
    assert_eq!(reference.semantic_digest, Some(sha256(&semantic)));
    assert_eq!(
        store
            .read(&reference.id, ArtifactComponent::Semantic)
            .unwrap(),
        semantic
    );
    drop(store);

    let reopened =
        FileArtifactStore::open_with_clock(root.path(), config(), controller_clock).unwrap();
    reopened.verify(&reference).unwrap();
    assert_eq!(reopened.find(&evaluation).unwrap(), Some(reference));
}

#[test]
fn expiry_removes_bytes_and_clock_rollback_cannot_restore_them() {
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let evaluation = ControllerEvaluationId::new("evaluation/expiry".to_owned()).unwrap();
    let retained = store
        .retain(
            &evaluation,
            ArtifactBundle {
                report: br#"{"schema":"amiss/report"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            },
        )
        .unwrap();

    clock.set(retained.expires_at_unix_millis);
    assert!(matches!(
        store.read(&retained.id, ArtifactComponent::Report),
        Err(ArtifactError::NotFound)
    ));
    assert_eq!(store.find(&evaluation).unwrap(), None);
    drop(store);

    clock.set(1_000);
    let reopened =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    assert_eq!(reopened.find(&evaluation).unwrap(), None);
}

#[test]
fn one_evaluation_cannot_be_rebound_and_missing_components_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let store = FileArtifactStore::open_with_clock(root.path(), config(), clock).unwrap();
    let evaluation = ControllerEvaluationId::new("evaluation/conflict".to_owned()).unwrap();
    let retained = store
        .retain(
            &evaluation,
            ArtifactBundle {
                report: br#"{"result":"first"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            },
        )
        .unwrap();

    assert!(matches!(
        store.retain(
            &evaluation,
            ArtifactBundle {
                report: br#"{"result":"second"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            }
        ),
        Err(ArtifactError::Conflict)
    ));
    assert!(matches!(
        store.read(&retained.id, ArtifactComponent::Assessment),
        Err(ArtifactError::NotFound)
    ));
}

#[test]
fn capacity_is_strict_without_eviction() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let mut limits = config();
    limits.max_records = 1;
    let store = FileArtifactStore::open_with_clock(root.path(), limits, clock).unwrap();
    let first = ControllerEvaluationId::new("evaluation/first".to_owned()).unwrap();
    let retained = store
        .retain(
            &first,
            ArtifactBundle {
                report: br#"{"result":"first"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            },
        )
        .unwrap();
    let second = ControllerEvaluationId::new("evaluation/second".to_owned()).unwrap();
    assert!(matches!(
        store.retain(
            &second,
            ArtifactBundle {
                report: br#"{"result":"second"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            }
        ),
        Err(ArtifactError::Full)
    ));
    assert_eq!(
        store.read(&retained.id, ArtifactComponent::Report).unwrap(),
        br#"{"result":"first"}"#
    );
}

#[test]
fn corrupted_payload_prevents_reopening_the_store() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&clock)).unwrap();
    let retained = store
        .retain(
            &ControllerEvaluationId::new("evaluation/corrupt".to_owned()).unwrap(),
            ArtifactBundle {
                report: br#"{"result":"exact"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            },
        )
        .unwrap();
    drop(store);
    std::fs::write(
        root.path().join(format!("{}.report", retained.id)),
        b"changed",
    )
    .unwrap();
    assert!(matches!(
        FileArtifactStore::open_with_clock(root.path(), config(), clock),
        Err(ArtifactError::Corrupt)
    ));
}

#[test]
fn one_oversized_record_is_not_misreported_as_recoverable_capacity() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let mut limits = config();
    limits.max_record_bytes = 128;
    let store = FileArtifactStore::open_with_clock(root.path(), limits, clock).unwrap();
    assert!(matches!(
        store.retain(
            &ControllerEvaluationId::new("evaluation/oversized".to_owned()).unwrap(),
            ArtifactBundle {
                report: br#"{"result":"too-large-for-this-record"}"#,
                semantic: None,
                plan: None,
                evidence: None,
                assessment: None,
                external_tally: None,
                external_incomplete: false,
            }
        ),
        Err(ArtifactError::TooLarge)
    ));
}
