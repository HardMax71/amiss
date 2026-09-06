#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid relation and provider identities"
)]

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ArtifactComponent, ArtifactStoreConfig, AuthenticatedDelivery, ChangeId, ChangeLocator,
    ControllerClock, ControllerEvaluationId, DeliveryId, DeliveryIdentity, FileArtifactStore,
    FileRelationScheduleStore, PendingRelation, ProviderError, ProviderRunAttempt, ProviderRunId,
    ProviderRunIdentity, RelationAcquiredRoot, RelationAcquisitionError, RelationAdmission,
    RelationCredentialRoute, RelationStatusDestination, RelationSubjectHead,
    RelationSubjectTransition, RelationTransition, TriggeredRelation, relation_credential_router,
    relation_registry, relation_transition,
};
use amiss_controller_fixtures::{clock::TestClock, relation::relation_audit};
use amiss_controller_service::{
    CoordinatedRelation, CoordinatedTransition, RelationAuditExecutionError, RelationAuditRequest,
    RelationOutboxError, drain_relation_outbox, execute_relation_audit, freeze_relation_transition,
};
use amiss_wire::controls::{BlobLineSelection, ProjectionKind, ProjectionSource};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepoPathText};
use amiss_wire::relation::{RelationSnapshot, RelationVerdict, parse_assessment};

struct RelationWorkFixture {
    documentation: amiss_fixtures::CommitPair,
    source: amiss_fixtures::CommitPair,
    transition: RelationTransition,
    report: Vec<u8>,
}

struct RelationStores {
    artifact_root: tempfile::TempDir,
    schedule_root: tempfile::TempDir,
    artifact_config: ArtifactStoreConfig,
    clock: Arc<dyn ControllerClock>,
    artifacts: FileArtifactStore,
    schedules: FileRelationScheduleStore,
}

fn delivery(transition: &RelationTransition) -> AuthenticatedDelivery {
    let subject = transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .unwrap();
    let frozen = transition
        .subjects
        .iter()
        .find(|frozen| frozen.role == subject.role)
        .unwrap();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: subject.scope.provider.clone(),
            integration: subject.scope.integration.clone(),
            delivery: DeliveryId::new("delivery/relation".to_owned()).unwrap(),
        },
        change: ChangeLocator {
            provider: subject.scope.provider.clone(),
            repository: subject.scope.repository.clone(),
            change: ChangeId::new("change/relation".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("run/relation".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            subject.object_format,
            frozen.commits.candidate.clone(),
        )
        .unwrap(),
    }
}

#[test]
fn either_authenticated_trigger_freezes_the_same_coordinated_revisions() {
    let source = relation_audit(false).unwrap().transition;
    let registry = relation_registry(vec![source.relation.plan.as_ref().clone()]).unwrap();
    let source_delivery = delivery(&source);
    assert_eq!(
        freeze_relation_transition(
            &registry,
            CoordinatedRelation {
                delivery: source_delivery.clone(),
                relation: source.relation.clone(),
                coordination: source.coordination.clone(),
            },
            source.subjects.clone(),
        ),
        Ok(CoordinatedTransition {
            delivery: source_delivery,
            transition: source.clone(),
        })
    );

    let mut documentation = source;
    documentation.relation.trigger_role = ArtifactId::new("documentation".to_owned()).unwrap();
    let documentation_delivery = delivery(&documentation);
    assert_eq!(
        freeze_relation_transition(
            &registry,
            CoordinatedRelation {
                delivery: documentation_delivery.clone(),
                relation: documentation.relation.clone(),
                coordination: documentation.coordination.clone(),
            },
            documentation.subjects.clone(),
        ),
        Ok(CoordinatedTransition {
            delivery: documentation_delivery,
            transition: documentation,
        })
    );
}

#[test]
fn an_authenticated_trigger_cannot_freeze_another_candidate() {
    let transition = relation_audit(false).unwrap().transition;
    let registry = relation_registry(vec![transition.relation.plan.as_ref().clone()]).unwrap();
    let delivery = delivery(&transition);
    let mut subjects = transition.subjects.clone();
    subjects
        .iter_mut()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .unwrap()
        .commits
        .candidate = Oid::new(ObjectFormat::Sha1, "0".repeat(40)).unwrap();

    assert_eq!(
        freeze_relation_transition(
            &registry,
            CoordinatedRelation {
                delivery,
                relation: transition.relation,
                coordination: transition.coordination,
            },
            subjects,
        ),
        Err(RelationAcquisitionError::InvalidTransition)
    );
}

#[test]
fn a_direct_stage_value_cannot_bypass_delivery_admission() {
    let transition = relation_audit(false).unwrap().transition;
    let registry = relation_registry(vec![transition.relation.plan.as_ref().clone()]).unwrap();
    let delivery = delivery(&transition);
    let mut forged = transition.relation.plan.as_ref().clone();
    forged.identity = ArtifactId::new("relation/unregistered".to_owned()).unwrap();

    assert_eq!(
        freeze_relation_transition(
            &registry,
            CoordinatedRelation {
                delivery,
                relation: TriggeredRelation {
                    plan: Arc::new(forged),
                    trigger_role: transition.relation.trigger_role,
                },
                coordination: transition.coordination,
            },
            transition.subjects,
        ),
        Err(RelationAcquisitionError::InvalidTransition)
    );
}

#[test]
fn current_relation_work_projects_assesses_retains_and_stages_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = relation_work()?;
    let stores = relation_stores(1)?;
    let RelationAdmission::Scheduled(pending) =
        stores.schedules.schedule(fixture.transition.clone())?
    else {
        return Err(std::io::Error::other("new relation work was not scheduled").into());
    };
    let evaluation_id = ControllerEvaluationId::new("evaluation/relation-service".to_owned())
        .ok_or_else(|| std::io::Error::other("invalid evaluation identity"))?;
    let staged = execute_relation_audit(
        &stores.artifacts,
        &stores.schedules,
        audit_request(&fixture, &pending, &evaluation_id),
    )?
    .ok_or_else(|| std::io::Error::other("current work did not stage"))?;

    assert_eq!(staged.targets.fence, pending.fence);
    assert_eq!(staged.targets.destinations.len(), 1);
    let assessment = stores.artifacts.read(
        &staged.audit.artifact.id,
        ArtifactComponent::RelationAssessment,
    )?;
    assert_eq!(
        parse_assessment(&assessment)?.payload.verdict,
        RelationVerdict::IntroducedDrift
    );
    assert!(
        !stores
            .artifacts
            .read(
                &staged.audit.artifact.id,
                ArtifactComponent::RelationEvidence,
            )?
            .is_empty()
    );
    assert_eq!(
        execute_relation_audit(
            &stores.artifacts,
            &stores.schedules,
            audit_request(&fixture, &pending, &evaluation_id),
        )?,
        Some(staged)
    );
    Ok(())
}

#[test]
fn superseded_relation_work_spends_no_projection_or_artifact_capacity()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = relation_work()?;
    let stores = relation_stores(2)?;
    let RelationAdmission::Scheduled(pending) =
        stores.schedules.schedule(fixture.transition.clone())?
    else {
        return Err(std::io::Error::other("new relation work was not scheduled").into());
    };
    let mut next = fixture.transition.clone();
    next.coordination = ArtifactId::new("workflow/release-next".to_owned())
        .ok_or_else(|| std::io::Error::other("invalid coordination identity"))?;
    assert!(matches!(
        stores.schedules.schedule(next)?,
        RelationAdmission::Scheduled(_)
    ));
    let evaluation_id = ControllerEvaluationId::new("evaluation/relation-stale".to_owned())
        .ok_or_else(|| std::io::Error::other("invalid evaluation identity"))?;

    assert!(matches!(
        execute_relation_audit(
            &stores.artifacts,
            &stores.schedules,
            audit_request(&fixture, &pending, &evaluation_id),
        ),
        Err(RelationAuditExecutionError::Superseded)
    ));
    assert_eq!(stores.artifacts.find(&evaluation_id)?, None);
    Ok(())
}

#[test]
fn relation_outbox_retries_after_restart_and_acknowledges_only_success()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = relation_work()?;
    let mut plan = fixture.transition.relation.plan.as_ref().clone();
    plan.status_destinations.push(RelationStatusDestination {
        subject_role: ArtifactId::new("source".to_owned()).unwrap(),
        required_status_name: "Amiss source relation".to_owned(),
    });
    fixture.transition.relation.plan = Arc::new(plan);
    let RelationStores {
        artifact_root,
        schedule_root,
        artifact_config,
        clock,
        artifacts,
        schedules,
    } = relation_stores(1)?;
    let RelationAdmission::Scheduled(pending) = schedules.schedule(fixture.transition.clone())?
    else {
        return Err(std::io::Error::other("new relation work was not scheduled").into());
    };
    let evaluation_id = ControllerEvaluationId::new("evaluation/relation-outbox".to_owned())
        .ok_or_else(|| std::io::Error::other("invalid evaluation identity"))?;
    let staged = execute_relation_audit(
        &artifacts,
        &schedules,
        audit_request(&fixture, &pending, &evaluation_id),
    )?
    .ok_or_else(|| std::io::Error::other("current work did not stage"))?;
    let relation = fixture.transition.relation.plan.identity.clone();
    let coordination = fixture.transition.coordination.clone();
    let registry = relation_registry(vec![fixture.transition.relation.plan.as_ref().clone()])?;
    let credentials = relation_credential_router(
        &registry,
        fixture
            .transition
            .relation
            .plan
            .subjects
            .iter()
            .map(|subject| RelationCredentialRoute {
                identity: subject.credential.clone(),
                authority: subject.role.clone(),
            })
            .collect(),
    )?;
    let expected_authorities = staged
        .targets
        .destinations
        .iter()
        .map(|target| target.role.clone())
        .collect::<Vec<_>>();

    drop(schedules);
    drop(artifacts);
    let artifacts = FileArtifactStore::open_with_clock(
        artifact_root.path(),
        artifact_config.clone(),
        Arc::clone(&clock),
    )?;
    let schedules = FileRelationScheduleStore::open(schedule_root.path(), 1)?;
    assert!(matches!(
        drain_relation_outbox(
            &registry,
            &credentials,
            &artifacts,
            &schedules,
            |authority, _status, target| {
                assert_eq!(authority, &target.role);
                Err(ProviderError::Unavailable)
            },
        ),
        Err(RelationOutboxError::Provider(ProviderError::Unavailable))
    ));
    assert_eq!(
        schedules.reopen_staged_status(&registry, &artifacts, &relation, &coordination)?,
        Some(staged.clone())
    );

    drop(schedules);
    drop(artifacts);
    let artifacts =
        FileArtifactStore::open_with_clock(artifact_root.path(), artifact_config, clock)?;
    let schedules = FileRelationScheduleStore::open(schedule_root.path(), 1)?;
    let mut delivered = Vec::new();
    drain_relation_outbox(
        &registry,
        &credentials,
        &artifacts,
        &schedules,
        |authority, status, target| {
            assert_eq!(status, &staged);
            assert_eq!(authority, &target.role);
            delivered.push(authority.clone());
            Ok(())
        },
    )?;
    delivered.sort();
    assert_eq!(delivered, expected_authorities);
    assert_eq!(
        schedules.reopen_staged_status(&registry, &artifacts, &relation, &coordination)?,
        None
    );
    Ok(())
}

fn relation_work() -> Result<RelationWorkFixture, Box<dyn std::error::Error>> {
    let documentation = amiss_fixtures::commit_pair(&[("projection.txt", "timeout: u64\n")], &[])?;
    let source = amiss_fixtures::commit_pair(
        &[("projection.txt", "timeout: u64\n")],
        &[("projection.txt", "timeout: u128\n")],
    )?;
    let original =
        relation_audit(false).ok_or_else(|| std::io::Error::other("invalid relation fixture"))?;
    let mut plan = original.transition.relation.plan.as_ref().clone();
    plan.projection = ProjectionKind::CodeTextV1;
    let path = RepoPathText::new("projection.txt".to_owned())
        .ok_or_else(|| std::io::Error::other("invalid projection path"))?;
    for subject in &mut plan.subjects {
        subject.source = ProjectionSource::BlobLines(BlobLineSelection {
            path: path.clone(),
            first_line: 1,
            last_line: 1,
        });
    }
    let transition = relation_transition(
        TriggeredRelation {
            plan: Arc::new(plan),
            trigger_role: original.transition.relation.trigger_role,
        },
        original.transition.coordination,
        [
            frozen("documentation", &documentation),
            frozen("source", &source),
        ],
    )?;
    let report = report_for(&original.report, &source)?;
    Ok(RelationWorkFixture {
        documentation,
        source,
        transition,
        report,
    })
}

fn frozen(role: &str, pair: &amiss_fixtures::CommitPair) -> RelationSubjectTransition {
    RelationSubjectTransition {
        role: ArtifactId::new(role.to_owned()).unwrap(),
        commits: amiss_controller::OidPair {
            base: Oid::new(ObjectFormat::Sha1, pair.base.clone()).unwrap(),
            candidate: Oid::new(ObjectFormat::Sha1, pair.candidate.clone()).unwrap(),
        },
        trees: amiss_controller::OidPair {
            base: Oid::new(ObjectFormat::Sha1, pair.base_tree.clone()).unwrap(),
            candidate: Oid::new(ObjectFormat::Sha1, pair.candidate_tree.clone()).unwrap(),
        },
    }
}

fn report_for(
    report: &[u8],
    source: &amiss_fixtures::CommitPair,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut report: serde_json::Value = serde_json::from_slice(report)?;
    let evaluation = report
        .pointer_mut("/payload/evaluation")
        .ok_or_else(|| std::io::Error::other("fixture report has no evaluation"))?;
    for (pointer, value) in [
        ("/base/commit_oid", source.base.as_str()),
        ("/base/tree_oid", source.base_tree.as_str()),
        ("/candidate/commit_oid", source.candidate.as_str()),
        ("/candidate/tree_oid", source.candidate_tree.as_str()),
    ] {
        *evaluation
            .pointer_mut(pointer)
            .ok_or_else(|| std::io::Error::other("fixture report lost one snapshot field"))? =
            serde_json::Value::String(value.to_owned());
    }
    let payload = report
        .pointer("/payload")
        .ok_or_else(|| std::io::Error::other("fixture report has no payload"))?;
    let payload_digest = amiss_wire::digest::hb(
        amiss_wire::report::PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(payload)?,
    );
    *report
        .pointer_mut("/payload_digest")
        .ok_or_else(|| std::io::Error::other("fixture report has no payload digest"))? =
        serde_json::Value::String(payload_digest.to_string());
    Ok(serde_json_canonicalizer::to_vec(&report)?)
}

fn relation_stores(max_bindings: u64) -> Result<RelationStores, Box<dyn std::error::Error>> {
    let artifact_root = tempfile::tempdir()?;
    let schedule_root = tempfile::tempdir()?;
    let clock: Arc<dyn ControllerClock> = TestClock::new();
    let artifact_config = ArtifactStoreConfig {
        base_url: "https://amiss.example/relation-artifacts".to_owned(),
        retention: Duration::from_hours(1),
        max_records: 4,
        max_bytes: 16 * 1_024 * 1_024,
        max_record_bytes: 4 * 1_024 * 1_024,
    };
    let artifacts = FileArtifactStore::open_with_clock(
        artifact_root.path(),
        artifact_config.clone(),
        Arc::clone(&clock),
    )?;
    let schedules = FileRelationScheduleStore::open(schedule_root.path(), max_bindings)?;
    Ok(RelationStores {
        artifact_root,
        schedule_root,
        artifact_config,
        clock,
        artifacts,
        schedules,
    })
}

fn audit_request<'a>(
    fixture: &'a RelationWorkFixture,
    pending: &'a PendingRelation,
    evaluation_id: &'a ControllerEvaluationId,
) -> RelationAuditRequest<'a> {
    let [documentation, source] = &pending.transition.subjects;
    RelationAuditRequest {
        evaluation_id,
        pending,
        report: &fixture.report,
        roots: [
            RelationAcquiredRoot {
                role: &documentation.role,
                repository: fixture.documentation.root(),
            },
            RelationAcquiredRoot {
                role: &source.role,
                repository: fixture.source.root(),
            },
        ],
        heads: pending.transition.subjects.each_ref().map(|frozen| {
            let subject = pending
                .transition
                .relation
                .plan
                .subjects
                .iter()
                .find(|subject| subject.role == frozen.role)
                .unwrap();
            RelationSubjectHead {
                subject: subject.clone(),
                candidate: RelationSnapshot {
                    commit: frozen.commits.candidate.clone(),
                    tree: frozen.trees.candidate.clone(),
                },
            }
        }),
        engine_version: env!("CARGO_PKG_VERSION"),
        engine_digest: sha256(b"relation service evaluator fixture"),
    }
}
