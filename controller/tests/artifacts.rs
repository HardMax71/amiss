use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactAuditBundle, ArtifactAuditDigests, ArtifactBundle, ArtifactComponent, ArtifactError,
    ArtifactStoreConfig, ControllerClock, ControllerEvaluationId, ExternalTally, FileArtifactStore,
    PublicationAuditBundle, RelationAuditBundle, validate_publication_audit,
    validate_relation_audit,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_fixtures::relation::relation_audit;
use amiss_controller_fixtures::semantic::semantic_input_artifact;
use amiss_fixtures::publication_audit;
use amiss_wire::digest::sha256;

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
    let fixture = semantic_input_artifact().unwrap();
    let report = fixture.report;
    let semantic = fixture.artifact;
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
fn publication_audits_survive_restart_with_optional_evidence_exact() {
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let mut retained = Vec::new();

    for (name, with_evidence) in [("matched", true), ("unproven", false)] {
        let fixture = publication_audit(with_evidence).unwrap();
        let bundle = PublicationAuditBundle {
            report: &fixture.report,
            plan: &fixture.plan,
            evidence: fixture.evidence.as_deref(),
            assessment: &fixture.assessment,
        };
        let evaluation =
            ControllerEvaluationId::new(format!("evaluation/publication/{name}")).unwrap();
        let expected = validate_publication_audit(bundle).unwrap();
        let reference = store
            .retain_audit(&evaluation, ArtifactAuditBundle::Publication(bundle))
            .unwrap();

        assert_eq!(reference.audit, ArtifactAuditDigests::Publication(expected));
        assert_eq!(
            store
                .retain_audit(&evaluation, ArtifactAuditBundle::Publication(bundle))
                .unwrap(),
            reference
        );
        assert_eq!(
            store
                .read(&reference.artifact.id, ArtifactComponent::PublicationPlan)
                .unwrap(),
            fixture.plan
        );
        assert_eq!(
            store
                .read(
                    &reference.artifact.id,
                    ArtifactComponent::PublicationAssessment,
                )
                .unwrap(),
            fixture.assessment
        );
        match fixture.evidence {
            Some(evidence) => assert_eq!(
                store
                    .read(
                        &reference.artifact.id,
                        ArtifactComponent::PublicationEvidence,
                    )
                    .unwrap(),
                evidence
            ),
            None => assert!(matches!(
                store.read(
                    &reference.artifact.id,
                    ArtifactComponent::PublicationEvidence,
                ),
                Err(ArtifactError::NotFound)
            )),
        }
        retained.push((evaluation, reference));
    }
    drop(store);

    let reopened =
        FileArtifactStore::open_with_clock(root.path(), config(), controller_clock).unwrap();
    for (evaluation, reference) in retained {
        reopened.verify(&reference.artifact).unwrap();
        assert_eq!(
            reopened.find(&evaluation).unwrap(),
            Some(reference.artifact)
        );
    }
}

#[test]
fn a_relation_audit_survives_restart_under_its_transition_binding() {
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let fixture = relation_audit(true).unwrap();
    let bundle = RelationAuditBundle {
        transition: &fixture.transition,
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    };
    let evaluation = ControllerEvaluationId::new("evaluation/relation".to_owned()).unwrap();
    let expected = validate_relation_audit(bundle).unwrap();
    let reference = store
        .retain_audit(&evaluation, ArtifactAuditBundle::Relation(bundle))
        .unwrap();

    assert_eq!(reference.audit, ArtifactAuditDigests::Relation(expected));
    assert_eq!(
        store
            .retain_audit(&evaluation, ArtifactAuditBundle::Relation(bundle))
            .unwrap(),
        reference
    );
    for (component, expected) in [
        (ArtifactComponent::RelationPlan, fixture.plan.as_slice()),
        (
            ArtifactComponent::RelationEvidence,
            fixture.evidence.as_deref().unwrap(),
        ),
        (
            ArtifactComponent::RelationAssessment,
            fixture.assessment.as_slice(),
        ),
    ] {
        assert_eq!(
            store.read(&reference.artifact.id, component).unwrap(),
            expected
        );
    }
    drop(store);

    let reopened =
        FileArtifactStore::open_with_clock(root.path(), config(), controller_clock).unwrap();
    reopened.verify(&reference.artifact).unwrap();
    assert_eq!(
        reopened.find(&evaluation).unwrap(),
        Some(reference.artifact)
    );
}

#[test]
fn an_invalid_publication_audit_creates_no_evaluation_binding() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let store = FileArtifactStore::open_with_clock(root.path(), config(), clock).unwrap();
    let evaluation =
        ControllerEvaluationId::new("evaluation/publication/invalid".to_owned()).unwrap();
    let mut fixture = publication_audit(true).unwrap();
    fixture.plan.push(b'x');

    assert!(matches!(
        store.retain_audit(
            &evaluation,
            ArtifactAuditBundle::Publication(PublicationAuditBundle {
                report: &fixture.report,
                plan: &fixture.plan,
                evidence: fixture.evidence.as_deref(),
                assessment: &fixture.assessment,
            })
        ),
        Err(ArtifactError::Corrupt)
    ));
    assert_eq!(store.find(&evaluation).unwrap(), None);
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
