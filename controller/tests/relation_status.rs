#![expect(
    clippy::unwrap_used,
    reason = "the fixtures construct known-valid relation audits and identities"
)]

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactAuditBundle, ArtifactAuditReference, ArtifactStoreConfig, ControllerClock,
    ControllerEvaluationId, FileArtifactStore, LeaseFence, PendingRelation, RelationAuditBundle,
    RelationStatusError, RelationStatusRecord, RelationSubjectHead, complete_relation_status,
    stage_relation_status,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_fixtures::relation::{RelationAuditFixture, relation_audit};
use amiss_wire::model::ArtifactId;

fn store(root: &tempfile::TempDir) -> FileArtifactStore {
    let clock: Arc<dyn ControllerClock> = TestClock::new();
    FileArtifactStore::open_with_clock(
        root.path(),
        ArtifactStoreConfig {
            base_url: "https://amiss.example/artifacts".to_owned(),
            retention: Duration::from_mins(1),
            max_records: 4,
            max_bytes: 8_388_608,
            max_record_bytes: 2_097_152,
        },
        clock,
    )
    .unwrap()
}

fn bundle(fixture: &RelationAuditFixture) -> RelationAuditBundle<'_> {
    RelationAuditBundle {
        transition: &fixture.transition,
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}

fn heads(fixture: &RelationAuditFixture) -> [RelationSubjectHead; 2] {
    fixture.transition.subjects.each_ref().map(|frozen| {
        let subject = fixture
            .transition
            .relation
            .plan
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .unwrap();
        RelationSubjectHead {
            subject: subject.clone(),
            candidate_commit: frozen.commits.candidate.clone(),
        }
    })
}

fn retain(
    store: &FileArtifactStore,
    evaluation: &str,
    fixture: &RelationAuditFixture,
) -> ArtifactAuditReference {
    store
        .retain_audit(
            &ControllerEvaluationId::new(evaluation.to_owned()).unwrap(),
            ArtifactAuditBundle::Relation(bundle(fixture)),
        )
        .unwrap()
}

#[test]
fn exact_status_stage_replays_and_completes_idempotently() {
    let fixture = relation_audit(true).unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let retained = retain(&store, "evaluation/relation/status", &fixture);
    let pending = PendingRelation {
        transition: fixture.transition.clone(),
        fence: LeaseFence::new(7).unwrap(),
    };

    let staged = stage_relation_status(
        &pending,
        Some(&pending),
        heads(&fixture),
        None,
        retained.clone(),
        bundle(&fixture),
    )
    .unwrap()
    .unwrap();
    assert!(!staged.completed);
    assert_eq!(staged.audit, retained);
    assert_eq!(staged.targets.fence, pending.fence);

    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&pending),
            heads(&fixture),
            Some(&staged),
            retained.clone(),
            bundle(&fixture),
        )
        .unwrap(),
        Some(staged.clone())
    );

    let completed = complete_relation_status(&staged, &staged).unwrap();
    assert!(completed.completed);
    assert_eq!(
        complete_relation_status(&completed, &staged).unwrap(),
        completed
    );
    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&pending),
            heads(&fixture),
            Some(&completed),
            retained,
            bundle(&fixture),
        )
        .unwrap(),
        None
    );
}

#[test]
fn stale_foreign_and_conflicting_status_state_fails_closed() {
    let fixture = relation_audit(true).unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = store(&root);
    let retained = retain(&store, "evaluation/relation/exact", &fixture);
    let pending = PendingRelation {
        transition: fixture.transition.clone(),
        fence: LeaseFence::new(7).unwrap(),
    };

    let stale = PendingRelation {
        transition: pending.transition.clone(),
        fence: LeaseFence::new(8).unwrap(),
    };
    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&stale),
            heads(&fixture),
            None,
            retained.clone(),
            bundle(&fixture),
        )
        .unwrap_err(),
        RelationStatusError::Superseded
    );

    let mut malformed = retained.clone();
    malformed.artifact.locator.push('x');
    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&pending),
            heads(&fixture),
            None,
            malformed,
            bundle(&fixture),
        )
        .unwrap_err(),
        RelationStatusError::InvalidAudit
    );

    let staged = stage_relation_status(
        &pending,
        Some(&pending),
        heads(&fixture),
        None,
        retained.clone(),
        bundle(&fixture),
    )
    .unwrap()
    .unwrap();
    let unproven = relation_audit(false).unwrap();
    let foreign = retain(&store, "evaluation/relation/foreign", &unproven);
    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&pending),
            heads(&fixture),
            Some(&staged),
            foreign.clone(),
            bundle(&unproven),
        )
        .unwrap_err(),
        RelationStatusError::BindingConflict
    );

    let conflicting = RelationStatusRecord {
        audit: foreign,
        ..staged.clone()
    };
    assert_eq!(
        complete_relation_status(&staged, &conflicting).unwrap_err(),
        RelationStatusError::BindingConflict
    );

    let mut foreign_transition = fixture.transition.clone();
    foreign_transition.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    assert_eq!(
        stage_relation_status(
            &pending,
            Some(&pending),
            heads(&fixture),
            None,
            retained,
            RelationAuditBundle {
                transition: &foreign_transition,
                report: &fixture.report,
                plan: &fixture.plan,
                evidence: fixture.evidence.as_deref(),
                assessment: &fixture.assessment,
            },
        )
        .unwrap_err(),
        RelationStatusError::InvalidAudit
    );
}
