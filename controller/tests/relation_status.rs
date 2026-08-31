#![expect(
    clippy::unwrap_used,
    reason = "the fixtures construct known-valid relation audits and identities"
)]

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactAuditBundle, ArtifactAuditReference, ArtifactError, ArtifactStoreConfig,
    ControllerClock, ControllerEvaluationId, FileArtifactStore, FileRelationScheduleStore,
    LeaseFence, PendingRelation, RelationAdmission, RelationAuditBundle, RelationRegistry,
    RelationScheduleError, RelationScheduleStoreError, RelationStatusDestination,
    RelationStatusError, RelationStatusRecord, RelationSubjectHead, complete_relation_status,
    relation_registry, stage_relation_status,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_fixtures::relation::{
    RelationAuditFixture, relation_audit, relation_audit_with_coordination,
};
use amiss_wire::model::ArtifactId;

fn store(root: &tempfile::TempDir, clock: Arc<dyn ControllerClock>) -> FileArtifactStore {
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

fn acknowledge_all(
    store: &FileRelationScheduleStore,
    registry: &RelationRegistry,
    artifacts: &FileArtifactStore,
) -> RelationStatusRecord {
    loop {
        let claim = store
            .claim_status_delivery(registry, artifacts)
            .unwrap()
            .unwrap();
        let status = store.acknowledge_status_destination(claim).unwrap();
        if status.completed {
            return status;
        }
    }
}

fn assert_journal_hides_credentials(path: &std::path::Path, fixture: &RelationAuditFixture) {
    let journal = std::fs::read(path).unwrap();
    assert!(
        fixture
            .transition
            .relation
            .plan
            .subjects
            .iter()
            .all(|subject| !journal
                .windows(subject.credential.as_str().len())
                .any(|window| window == subject.credential.as_str().as_bytes()))
    );
}

#[test]
fn exact_status_stage_replays_and_completes_idempotently() {
    let fixture = relation_audit(true).unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = store(&root, TestClock::new());
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
    let store = store(&root, TestClock::new());
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

#[test]
fn durable_status_replays_and_completes_exactly_across_restart() {
    let mut fixture = relation_audit(true).unwrap();
    Arc::make_mut(&mut fixture.transition.relation.plan)
        .status_destinations
        .push(RelationStatusDestination {
            subject_role: ArtifactId::new("source".to_owned()).unwrap(),
            required_status_name: "Amiss source relation".to_owned(),
        });
    let registry =
        relation_registry(vec![fixture.transition.relation.plan.as_ref().clone()]).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let artifacts = store(&artifact_root, TestClock::new());
    let retained = retain(&artifacts, "evaluation/relation/durable", &fixture);
    let relation_root = tempfile::tempdir().unwrap();
    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    let RelationAdmission::Scheduled(pending) =
        relations.schedule(fixture.transition.clone()).unwrap()
    else {
        panic!("the exact relation schedules");
    };

    let staged = relations
        .stage_status(
            &artifacts,
            &pending,
            heads(&fixture),
            retained.clone(),
            bundle(&fixture),
        )
        .unwrap()
        .unwrap();
    let journal_path = relation_root
        .path()
        .join(".amiss-relation-schedules.journal");
    assert_journal_hides_credentials(&journal_path, &fixture);
    drop(relations);

    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    assert_eq!(
        relations
            .reopen_staged_status(
                &registry,
                &artifacts,
                &staged.targets.relation,
                &staged.targets.coordination,
            )
            .unwrap(),
        Some(staged.clone())
    );
    assert!(matches!(
        relations.complete_status(&staged),
        Err(RelationScheduleStoreError::DeliveryPending)
    ));
    let first = relations
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert_eq!(first.status, staged);
    let first_target = first.target.clone();
    let parallel = relations
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert_ne!(parallel.target, first_target);
    drop(parallel);
    assert!(
        !relations
            .acknowledge_status_destination(first)
            .unwrap()
            .completed
    );
    drop(relations);

    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    let second = relations
        .claim_status_delivery(&registry, &artifacts)
        .unwrap();
    let second = second.unwrap();
    assert_ne!(second.target, first_target);
    assert!(matches!(
        relations.complete_status(&staged),
        Err(RelationScheduleStoreError::DeliveryPending)
    ));
    let completed = relations.acknowledge_status_destination(second).unwrap();
    assert!(completed.completed);
    drop(relations);

    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    assert_eq!(relations.complete_status(&staged).unwrap(), completed);
    assert_eq!(
        relations
            .reopen_staged_status(
                &registry,
                &artifacts,
                &staged.targets.relation,
                &staged.targets.coordination,
            )
            .unwrap(),
        None
    );
}

#[test]
fn delivery_claim_recovers_the_oldest_fence_before_newer_work() {
    let older = relation_audit(true).unwrap();
    let newer = relation_audit_with_coordination(true, "workflow/release-43").unwrap();
    let registry =
        relation_registry(vec![older.transition.relation.plan.as_ref().clone()]).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let artifacts = store(&artifact_root, TestClock::new());
    let older_audit = retain(&artifacts, "evaluation/relation/delivery-older", &older);
    let newer_audit = retain(&artifacts, "evaluation/relation/delivery-newer", &newer);
    let relation_root = tempfile::tempdir().unwrap();
    let relations = FileRelationScheduleStore::open(relation_root.path(), 2).unwrap();
    let contender = FileRelationScheduleStore::open(relation_root.path(), 2).unwrap();
    let RelationAdmission::Scheduled(older_pending) =
        relations.schedule(older.transition.clone()).unwrap()
    else {
        panic!("the older relation schedules");
    };
    let older_status = relations
        .stage_status(
            &artifacts,
            &older_pending,
            heads(&older),
            older_audit,
            bundle(&older),
        )
        .unwrap()
        .unwrap();
    let RelationAdmission::Scheduled(newer_pending) =
        relations.schedule(newer.transition.clone()).unwrap()
    else {
        panic!("the newer relation schedules");
    };
    let newer_status = relations
        .stage_status(
            &artifacts,
            &newer_pending,
            heads(&newer),
            newer_audit,
            bundle(&newer),
        )
        .unwrap()
        .unwrap();

    let claim = relations
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert_eq!(claim.status, older_status);
    let target = claim.target.clone();
    assert!(
        contender
            .claim_status_delivery(&registry, &artifacts)
            .unwrap()
            .is_none()
    );
    drop(claim);

    let mut recovered = contender
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, older_status);
    assert_eq!(recovered.target, target);
    recovered.target.credential =
        amiss_controller::OpaqueId::new("credential/rebound".to_owned()).unwrap();
    assert!(matches!(
        contender.acknowledge_status_destination(recovered),
        Err(RelationScheduleStoreError::Status(
            RelationStatusError::BindingConflict
        ))
    ));
    let mut recovered = contender
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    recovered.status.targets.fence = LeaseFence::new(99).unwrap();
    assert!(matches!(
        contender.acknowledge_status_destination(recovered),
        Err(RelationScheduleStoreError::Status(
            RelationStatusError::BindingConflict
        ))
    ));
    let recovered = contender
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert!(
        contender
            .acknowledge_status_destination(recovered)
            .unwrap()
            .completed
    );
    let next = relations
        .claim_status_delivery(&registry, &artifacts)
        .unwrap()
        .unwrap();
    assert_eq!(next.status, newer_status);
    assert!(
        relations
            .acknowledge_status_destination(next)
            .unwrap()
            .completed
    );
}

#[test]
fn reopening_status_rejects_missing_rebound_and_expired_authorities() {
    let fixture = relation_audit(true).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let clock = TestClock::new();
    let artifacts = store(&artifact_root, clock.clone());
    let retained = retain(&artifacts, "evaluation/relation/reopen", &fixture);
    let relation_root = tempfile::tempdir().unwrap();
    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    let RelationAdmission::Scheduled(pending) =
        relations.schedule(fixture.transition.clone()).unwrap()
    else {
        panic!("the exact relation schedules");
    };
    let staged = relations
        .stage_status(
            &artifacts,
            &pending,
            heads(&fixture),
            retained,
            bundle(&fixture),
        )
        .unwrap()
        .unwrap();

    let empty = relation_registry(Vec::new()).unwrap();
    assert!(matches!(
        relations.reopen_staged_status(
            &empty,
            &artifacts,
            &staged.targets.relation,
            &staged.targets.coordination,
        ),
        Err(RelationScheduleStoreError::Configuration)
    ));

    let mut credential_rebound = fixture.transition.relation.plan.as_ref().clone();
    credential_rebound.subjects[0].credential =
        amiss_controller::OpaqueId::new("credential/rebound".to_owned()).unwrap();
    let mut limits_rebound = fixture.transition.relation.plan.as_ref().clone();
    limits_rebound.aggregate_limits.acquisition_objects += 1;
    for rebound in [credential_rebound, limits_rebound] {
        let rebound = relation_registry(vec![rebound]).unwrap();
        assert!(matches!(
            relations.reopen_staged_status(
                &rebound,
                &artifacts,
                &staged.targets.relation,
                &staged.targets.coordination,
            ),
            Err(RelationScheduleStoreError::Schedule(
                RelationScheduleError::BindingConflict
            ))
        ));
    }

    let registry =
        relation_registry(vec![fixture.transition.relation.plan.as_ref().clone()]).unwrap();
    clock.advance(60_000);
    assert!(matches!(
        relations.reopen_staged_status(
            &registry,
            &artifacts,
            &staged.targets.relation,
            &staged.targets.coordination,
        ),
        Err(RelationScheduleStoreError::Artifact(
            ArtifactError::NotFound
        ))
    ));
}

#[test]
fn status_rebinding_and_superseded_staging_do_not_change_the_journal() {
    let fixture = relation_audit(true).unwrap();
    let registry =
        relation_registry(vec![fixture.transition.relation.plan.as_ref().clone()]).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let artifacts = store(&artifact_root, TestClock::new());
    let retained = retain(&artifacts, "evaluation/relation/exact-status", &fixture);
    let relation_root = tempfile::tempdir().unwrap();
    let relations = FileRelationScheduleStore::open(relation_root.path(), 2).unwrap();
    let RelationAdmission::Scheduled(pending) =
        relations.schedule(fixture.transition.clone()).unwrap()
    else {
        panic!("the exact relation schedules");
    };
    let staged = relations
        .stage_status(
            &artifacts,
            &pending,
            heads(&fixture),
            retained,
            bundle(&fixture),
        )
        .unwrap()
        .unwrap();

    let foreign_fixture = relation_audit(false).unwrap();
    let foreign = retain(
        &artifacts,
        "evaluation/relation/foreign-status",
        &foreign_fixture,
    );
    let journal = relation_root
        .path()
        .join(".amiss-relation-schedules.journal");
    let before_conflict = std::fs::metadata(&journal).unwrap().len();
    assert!(matches!(
        relations.stage_status(
            &artifacts,
            &pending,
            heads(&foreign_fixture),
            foreign,
            bundle(&foreign_fixture),
        ),
        Err(RelationScheduleStoreError::Status(
            RelationStatusError::BindingConflict
        ))
    ));
    assert_eq!(std::fs::metadata(&journal).unwrap().len(), before_conflict);

    let mut next = fixture.transition.clone();
    next.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    let RelationAdmission::Scheduled(next) = relations.schedule(next).unwrap() else {
        panic!("the next coordination schedules");
    };
    let before_stale = std::fs::metadata(&journal).unwrap().len();
    assert!(matches!(
        relations.stage_status(
            &artifacts,
            &pending,
            heads(&fixture),
            staged.audit.clone(),
            bundle(&fixture),
        ),
        Err(RelationScheduleStoreError::Status(
            RelationStatusError::Superseded
        ))
    ));
    assert_eq!(std::fs::metadata(&journal).unwrap().len(), before_stale);
    assert!(relations.is_current(&next).unwrap());
    assert!(acknowledge_all(&relations, &registry, &artifacts).completed);
    assert!(relations.complete_status(&staged).unwrap().completed);
}

#[test]
fn an_expired_audit_cannot_create_a_status_stage() {
    let fixture = relation_audit(true).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let clock = TestClock::new();
    let artifacts = store(&artifact_root, clock.clone());
    let retained = retain(&artifacts, "evaluation/relation/expired-status", &fixture);
    let relation_root = tempfile::tempdir().unwrap();
    let relations = FileRelationScheduleStore::open(relation_root.path(), 1).unwrap();
    let RelationAdmission::Scheduled(pending) =
        relations.schedule(fixture.transition.clone()).unwrap()
    else {
        panic!("the exact relation schedules");
    };
    let journal = relation_root
        .path()
        .join(".amiss-relation-schedules.journal");
    let scheduled_bytes = std::fs::metadata(&journal).unwrap().len();

    clock.advance(60_000);
    assert!(matches!(
        relations.stage_status(
            &artifacts,
            &pending,
            heads(&fixture),
            retained,
            bundle(&fixture),
        ),
        Err(RelationScheduleStoreError::Artifact(
            ArtifactError::NotFound
        ))
    ));
    assert_eq!(std::fs::metadata(journal).unwrap().len(), scheduled_bytes);
}

#[test]
fn concurrent_supersession_and_status_staging_commit_in_one_order() {
    let fixture = relation_audit(true).unwrap();
    let registry =
        relation_registry(vec![fixture.transition.relation.plan.as_ref().clone()]).unwrap();
    let artifact_root = tempfile::tempdir().unwrap();
    let artifacts = Arc::new(store(&artifact_root, TestClock::new()));
    let retained = retain(
        artifacts.as_ref(),
        "evaluation/relation/concurrent-status",
        &fixture,
    );
    let relation_root = tempfile::tempdir().unwrap();
    let stores = [
        FileRelationScheduleStore::open(relation_root.path(), 2).unwrap(),
        FileRelationScheduleStore::open(relation_root.path(), 2).unwrap(),
    ];
    let RelationAdmission::Scheduled(pending) =
        stores[0].schedule(fixture.transition.clone()).unwrap()
    else {
        panic!("the exact relation schedules");
    };
    let mut next = fixture.transition.clone();
    next.coordination = ArtifactId::new("workflow/release-43".to_owned()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));

    let stage_barrier = Arc::clone(&barrier);
    let stage_artifacts = Arc::clone(&artifacts);
    let stage_store = stores[0].clone();
    let stage_worker = std::thread::spawn(move || {
        stage_barrier.wait();
        stage_store.stage_status(
            stage_artifacts.as_ref(),
            &pending,
            heads(&fixture),
            retained,
            bundle(&fixture),
        )
    });
    let schedule_barrier = Arc::clone(&barrier);
    let schedule_store = stores[1].clone();
    let schedule_worker = std::thread::spawn(move || {
        schedule_barrier.wait();
        schedule_store.schedule(next)
    });
    barrier.wait();

    let RelationAdmission::Scheduled(current) = schedule_worker.join().unwrap().unwrap() else {
        panic!("distinct work schedules");
    };
    let stage = stage_worker.join().unwrap();
    if let Ok(Some(staged)) = &stage {
        assert!(acknowledge_all(&stores[0], &registry, artifacts.as_ref()).completed);
        assert!(stores[0].complete_status(staged).unwrap().completed);
    }
    assert!(matches!(
        stage,
        Ok(Some(_))
            | Err(RelationScheduleStoreError::Status(
                RelationStatusError::Superseded
            ))
    ));
    assert!(stores[0].is_current(&current).unwrap());
}
