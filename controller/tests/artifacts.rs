use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactAuditBundle, ArtifactAuditDigests, ArtifactAuditReference, ArtifactBundle,
    ArtifactComponent, ArtifactError, ArtifactStoreConfig, ControllerClock, ControllerEvaluationId,
    ExternalTally, FileArtifactStore, LocaleAuditBundle, PublicationAuditBundle,
    RelationAuditBundle, validate_locale_audit, validate_publication_audit,
    validate_relation_audit,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_fixtures::relation::relation_audit;
use amiss_fixtures::{locale_audit, publication_audit};

fn config() -> ArtifactStoreConfig {
    ArtifactStoreConfig {
        base_url: "https://amiss.example/artifacts".to_owned(),
        retention: Duration::from_secs(1),
        max_records: 6,
        max_bytes: 2_097_152,
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

/// One sidecar's three stored components in the order the record holds them.
fn parts<'a>(
    plan: &'a [u8],
    evidence: Option<&'a [u8]>,
    assessment: &'a [u8],
    names: [ArtifactComponent; 3],
) -> [(ArtifactComponent, Option<&'a [u8]>); 3] {
    let [plan_name, evidence_name, assessment_name] = names;
    [
        (plan_name, Some(plan)),
        (evidence_name, evidence),
        (assessment_name, Some(assessment)),
    ]
}

/// Retains one audit twice, proving the second is the same record, and reads
/// back every component the sidecar should hold.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn retained_once(
    store: &FileArtifactStore,
    evaluation: ControllerEvaluationId,
    bundle: ArtifactAuditBundle<'_>,
    expected: ArtifactAuditDigests,
    components: [(ArtifactComponent, Option<&[u8]>); 3],
) -> (ControllerEvaluationId, ArtifactAuditReference) {
    let reference = store.retain_audit(&evaluation, bundle).unwrap();
    assert_eq!(reference.audit, expected);
    assert_eq!(store.retain_audit(&evaluation, bundle).unwrap(), reference);
    for (component, expected) in components {
        match expected {
            Some(expected) => assert_eq!(
                store.read(&reference.artifact.id, component).unwrap(),
                expected
            ),
            None => assert!(matches!(
                store.read(&reference.artifact.id, component),
                Err(ArtifactError::NotFound)
            )),
        }
    }
    (evaluation, reference)
}

#[test]
fn audits_survive_restart_with_optional_evidence_exact() {
    let root = tempfile::tempdir().unwrap();
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let store =
        FileArtifactStore::open_with_clock(root.path(), config(), Arc::clone(&controller_clock))
            .unwrap();
    let mut retained = Vec::new();

    for (mode, with_evidence) in [("evidence", true), ("unproven", false)] {
        let publication = publication_audit(with_evidence).unwrap();
        let publication_bundle = PublicationAuditBundle {
            report: &publication.report,
            plan: &publication.plan,
            evidence: publication.evidence.as_deref(),
            assessment: &publication.assessment,
        };
        let locale = locale_audit(with_evidence).unwrap();
        let locale_bundle = LocaleAuditBundle {
            report: &locale.report,
            plan: &locale.plan,
            evidence: locale.evidence.as_deref(),
            assessment: &locale.assessment,
        };
        let relation = relation_audit(with_evidence).unwrap();
        let relation_bundle = RelationAuditBundle {
            transition: &relation.transition,
            report: &relation.report,
            plan: &relation.plan,
            evidence: relation.evidence.as_deref(),
            assessment: &relation.assessment,
        };

        for (kind, bundle, expected, components) in [
            (
                "publication",
                ArtifactAuditBundle::Publication(publication_bundle),
                ArtifactAuditDigests::Publication(
                    validate_publication_audit(publication_bundle).unwrap(),
                ),
                parts(
                    &publication.plan,
                    publication.evidence.as_deref(),
                    &publication.assessment,
                    [
                        ArtifactComponent::PublicationPlan,
                        ArtifactComponent::PublicationEvidence,
                        ArtifactComponent::PublicationAssessment,
                    ],
                ),
            ),
            (
                "locale",
                ArtifactAuditBundle::Locale(locale_bundle),
                ArtifactAuditDigests::Locale(validate_locale_audit(locale_bundle).unwrap()),
                parts(
                    &locale.plan,
                    locale.evidence.as_deref(),
                    &locale.assessment,
                    [
                        ArtifactComponent::LocalePlan,
                        ArtifactComponent::LocaleEvidence,
                        ArtifactComponent::LocaleAssessment,
                    ],
                ),
            ),
            (
                "relation",
                ArtifactAuditBundle::Relation(relation_bundle),
                ArtifactAuditDigests::Relation(validate_relation_audit(relation_bundle).unwrap()),
                parts(
                    &relation.plan,
                    relation.evidence.as_deref(),
                    &relation.assessment,
                    [
                        ArtifactComponent::RelationPlan,
                        ArtifactComponent::RelationEvidence,
                        ArtifactComponent::RelationAssessment,
                    ],
                ),
            ),
        ] {
            let evaluation =
                ControllerEvaluationId::new(format!("evaluation/{kind}/{mode}")).unwrap();
            retained.push(retained_once(
                &store, evaluation, bundle, expected, components,
            ));
        }
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
fn invalid_audits_create_no_evaluation_binding() {
    let root = tempfile::tempdir().unwrap();
    let clock: Arc<dyn ControllerClock> = TestClock::at(1_000);
    let store = FileArtifactStore::open_with_clock(root.path(), config(), clock).unwrap();
    let publication_evaluation =
        ControllerEvaluationId::new("evaluation/publication/invalid".to_owned()).unwrap();
    let mut publication = publication_audit(true).unwrap();
    publication.plan.push(b'x');

    assert!(matches!(
        store.retain_audit(
            &publication_evaluation,
            ArtifactAuditBundle::Publication(PublicationAuditBundle {
                report: &publication.report,
                plan: &publication.plan,
                evidence: publication.evidence.as_deref(),
                assessment: &publication.assessment,
            })
        ),
        Err(ArtifactError::Corrupt)
    ));

    let relation_evaluation =
        ControllerEvaluationId::new("evaluation/relation/invalid".to_owned()).unwrap();
    let mut relation = relation_audit(true).unwrap();
    relation.plan.push(b'x');
    assert!(matches!(
        store.retain_audit(
            &relation_evaluation,
            ArtifactAuditBundle::Relation(RelationAuditBundle {
                transition: &relation.transition,
                report: &relation.report,
                plan: &relation.plan,
                evidence: relation.evidence.as_deref(),
                assessment: &relation.assessment,
            })
        ),
        Err(ArtifactError::Corrupt)
    ));

    assert_eq!(store.find(&publication_evaluation).unwrap(), None);
    assert_eq!(store.find(&relation_evaluation).unwrap(), None);
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
