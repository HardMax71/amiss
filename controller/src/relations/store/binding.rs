use amiss_wire::controls::projection_source_value;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::model::ArtifactId;
use serde::Serialize;

use super::super::{
    PendingRelation, RelationLimits, RelationPlan, RelationScheduleError, RelationTransition,
    TriggeredRelation, relation_transition,
};
use super::{RelationScheduleStoreError, StoredBinding};
use crate::LeaseFence;

const PLAN_BINDING_DOMAIN: &str = "amiss/controller-relation-plan-binding-v1";
const SOURCE_BINDING_SCHEMA: &str = "amiss/controller-relation-source-binding-v1";
const WORK_BINDING_DOMAIN: &str = "amiss/controller-relation-work-binding-v1";

pub(super) struct CheckedWork {
    pub(super) transition: RelationTransition,
    pub(super) relation: String,
    pub(super) plan_binding: String,
    pub(super) binding: StoredBinding,
}

#[derive(Serialize)]
struct BoundLimits {
    acquisition_objects: u64,
    acquisition_bytes: u64,
    projection_records: u64,
    projection_bytes: u64,
}

#[derive(Serialize)]
struct BoundPlanSubject<'a> {
    role: &'a str,
    provider_namespace: &'a str,
    provider_instance: &'a str,
    integration: &'a str,
    repository_host: &'a str,
    repository_owner: &'a str,
    repository_name: &'a str,
    target: &'a str,
    object_format: &'a str,
    credential: &'a str,
    source: String,
    limits: BoundLimits,
}

#[derive(Serialize)]
struct BoundStatus<'a> {
    subject_role: &'a str,
    required_status_name: &'a str,
}

#[derive(Serialize)]
struct BoundPlan<'a> {
    identity: &'a str,
    context_digest: String,
    projection: &'a str,
    subjects: [BoundPlanSubject<'a>; 2],
    aggregate_limits: BoundLimits,
    status_destinations: Vec<BoundStatus<'a>>,
}

#[derive(Serialize)]
struct BoundWorkSubject<'a> {
    role: &'a str,
    object_format: String,
    base_commit: &'a str,
    candidate_commit: &'a str,
    base_tree: &'a str,
    candidate_tree: &'a str,
}

#[derive(Serialize)]
struct BoundWork<'a> {
    relation: &'a str,
    plan_binding: &'a str,
    coordination: &'a str,
    subjects: [BoundWorkSubject<'a>; 2],
}

pub(super) fn checked_work(
    transition: RelationTransition,
) -> Result<CheckedWork, RelationScheduleStoreError> {
    let transition = relation_transition(
        transition.relation,
        transition.coordination,
        transition.subjects,
    )
    .map_err(|_defect| {
        RelationScheduleStoreError::Schedule(RelationScheduleError::InvalidTransition)
    })?;
    let plan_binding = plan_binding(transition.relation.plan.as_ref())?.to_string();
    let relation = transition.relation.plan.identity.as_str().to_owned();
    let subjects = transition
        .subjects
        .each_ref()
        .map(|subject| BoundWorkSubject {
            role: subject.role.as_str(),
            object_format: subject.commits.base.object_format().as_ref().to_owned(),
            base_commit: subject.commits.base.as_str(),
            candidate_commit: subject.commits.candidate.as_str(),
            base_tree: subject.trees.base.as_str(),
            candidate_tree: subject.trees.candidate.as_str(),
        });
    let bytes = serde_json::to_vec(&BoundWork {
        relation: &relation,
        plan_binding: &plan_binding,
        coordination: transition.coordination.as_str(),
        subjects,
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let binding = StoredBinding {
        coordination: transition.coordination.as_str().to_owned(),
        work_binding: hb(WORK_BINDING_DOMAIN, &bytes).to_string(),
        trigger_role: transition.relation.trigger_role.as_str().to_owned(),
        fence: 0,
    };
    Ok(CheckedWork {
        transition,
        relation,
        plan_binding,
        binding,
    })
}

pub(super) fn pending_from_binding(
    transition: RelationTransition,
    binding: &StoredBinding,
) -> Result<PendingRelation, RelationScheduleStoreError> {
    let relation = TriggeredRelation {
        plan: transition.relation.plan,
        trigger_role: ArtifactId::new(binding.trigger_role.clone())
            .ok_or(RelationScheduleStoreError::Corrupt)?,
    };
    let transition = relation_transition(
        relation,
        ArtifactId::new(binding.coordination.clone()).ok_or(RelationScheduleStoreError::Corrupt)?,
        transition.subjects,
    )
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    Ok(PendingRelation {
        transition,
        fence: LeaseFence::new(binding.fence).ok_or(RelationScheduleStoreError::Corrupt)?,
    })
}

pub(super) fn plan_binding(plan: &RelationPlan) -> Result<Digest, RelationScheduleStoreError> {
    let limits = |limits: RelationLimits| BoundLimits {
        acquisition_objects: limits.acquisition_objects,
        acquisition_bytes: limits.acquisition_bytes,
        projection_records: limits.projection_records,
        projection_bytes: limits.projection_bytes,
    };
    let subjects = plan.subjects.each_ref().map(|subject| BoundPlanSubject {
        role: subject.role.as_str(),
        provider_namespace: subject.scope.provider.namespace.as_str(),
        provider_instance: subject.scope.provider.instance.as_str(),
        integration: subject.scope.integration.as_str(),
        repository_host: subject.scope.repository.host(),
        repository_owner: subject.scope.repository.owner(),
        repository_name: subject.scope.repository.name(),
        target: subject.target.as_str(),
        object_format: subject.object_format.as_ref(),
        credential: subject.credential.as_str(),
        source: hj(
            SOURCE_BINDING_SCHEMA,
            &projection_source_value(&subject.source),
        )
        .to_string(),
        limits: limits(subject.limits),
    });
    let status_destinations = plan
        .status_destinations
        .iter()
        .map(|destination| BoundStatus {
            subject_role: destination.subject_role.as_str(),
            required_status_name: &destination.required_status_name,
        })
        .collect();
    let bytes = serde_json::to_vec(&BoundPlan {
        identity: plan.identity.as_str(),
        context_digest: plan.context_digest.to_string(),
        projection: plan.projection.as_ref(),
        subjects,
        aggregate_limits: limits(plan.aggregate_limits),
        status_destinations,
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    Ok(hb(PLAN_BINDING_DOMAIN, &bytes))
}
