mod tests;

use amiss_controller::{
    RelationAcquiredRoot, RelationTransition, relation_transition, verify_relation_acquired,
};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{RepositoryProjectionLimits, RepositoryProjectionRequest, project_repository};
use amiss_wire::json::Value;
use amiss_wire::relation::{
    RelationEvidence, RelationEvidenceSubject, RelationPlanEnvelope, evidence, plan,
};

#[derive(Clone, Copy)]
pub struct RelationProjectionRequest<'a> {
    pub transition: &'a RelationTransition,
    pub plan: &'a RelationPlanEnvelope,
    pub roots: [RelationAcquiredRoot<'a>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationProjectionError {
    #[error("the relation audit plan does not reproduce the frozen transition")]
    InvalidPlan,
    #[error("the acquired relation cannot prove all selected projection work")]
    Unproven,
}

/// Revalidates two acquired roots and projects all four exact snapshots into
/// one plan-bound evidence document.
///
/// # Errors
///
/// The plan differs from the frozen operator relation, an acquired root or
/// object cannot be trusted, a projection budget is crossed, or the evidence
/// document cannot reproduce the checked result.
pub fn project_relation_evidence(
    request: RelationProjectionRequest<'_>,
) -> Result<Value, RelationProjectionError> {
    let rebuilt =
        plan(&request.plan.payload).map_err(|_defect| RelationProjectionError::InvalidPlan)?;
    if rebuilt.text("payload_digest") != Some(&request.plan.payload_digest.to_string()) {
        return Err(RelationProjectionError::InvalidPlan);
    }
    let transition = relation_transition(
        request.transition.relation.clone(),
        request.transition.subjects.clone(),
    )
    .map_err(|_defect| RelationProjectionError::InvalidPlan)?;
    if !plan_matches_transition(request.plan, &transition) {
        return Err(RelationProjectionError::InvalidPlan);
    }
    let roots = verify_relation_acquired(&transition, request.roots)
        .map_err(|_defect| RelationProjectionError::Unproven)?;
    let registered = transition.relation.plan.as_ref();
    let mut aggregate_records = registered.aggregate_limits.projection_records;
    let mut aggregate_bytes = registered.aggregate_limits.projection_bytes;
    let mut subjects = Vec::with_capacity(transition.subjects.len());

    for ((root, frozen), planned) in roots
        .into_iter()
        .zip(&transition.subjects)
        .zip(&request.plan.payload.subjects)
    {
        let subject = registered
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .ok_or(RelationProjectionError::InvalidPlan)?;
        let repository = Repository::open(root.repository, subject.object_format)
            .map_err(|_defect| RelationProjectionError::Unproven)?;
        let mut git = GitResources::new(GitLimits::CONTRACT);
        let mut subject_records = subject.limits.projection_records;
        let mut subject_bytes = subject.limits.projection_bytes;
        let values = [&frozen.trees.base, &frozen.trees.candidate]
            .into_iter()
            .map(|tree| {
                let outcome = project_repository(RepositoryProjectionRequest {
                    repository: &repository,
                    git: &mut git,
                    tree,
                    projection: registered.projection,
                    source: &subject.source,
                    limits: RepositoryProjectionLimits {
                        records: subject_records.min(aggregate_records),
                        bytes: subject_bytes.min(aggregate_bytes),
                    },
                })
                .map_err(|_defect| RelationProjectionError::Unproven)?;
                subject_records = subject_records
                    .checked_sub(outcome.records)
                    .ok_or(RelationProjectionError::Unproven)?;
                subject_bytes = subject_bytes
                    .checked_sub(outcome.bytes)
                    .ok_or(RelationProjectionError::Unproven)?;
                aggregate_records = aggregate_records
                    .checked_sub(outcome.records)
                    .ok_or(RelationProjectionError::Unproven)?;
                aggregate_bytes = aggregate_bytes
                    .checked_sub(outcome.bytes)
                    .ok_or(RelationProjectionError::Unproven)?;
                Ok(outcome.value)
            })
            .collect::<Result<Vec<_>, RelationProjectionError>>()?;
        let [base, candidate] = values
            .try_into()
            .map_err(|_values: Vec<_>| RelationProjectionError::Unproven)?;
        subjects.push(RelationEvidenceSubject {
            role: planned.role.clone(),
            base,
            candidate,
        });
    }

    evidence(&RelationEvidence {
        plan_payload_digest: request.plan.payload_digest,
        subjects: subjects
            .try_into()
            .map_err(|_subjects: Vec<_>| RelationProjectionError::InvalidPlan)?,
    })
    .map_err(|_defect| RelationProjectionError::Unproven)
}

fn plan_matches_transition(plan: &RelationPlanEnvelope, transition: &RelationTransition) -> bool {
    let registered = transition.relation.plan.as_ref();
    plan.payload.relation.identity == registered.identity
        && plan.payload.trigger_role == transition.relation.trigger_role
        && plan.payload.projection == registered.projection
        && plan.payload.subjects.iter().all(|planned| {
            registered
                .subjects
                .iter()
                .find(|subject| subject.role == planned.role)
                .zip(
                    transition
                        .subjects
                        .iter()
                        .find(|subject| subject.role == planned.role),
                )
                .is_some_and(|(subject, frozen)| {
                    planned.repository == subject.scope.repository
                        && planned.target == subject.target
                        && planned.source == subject.source
                        && planned.base.commit == frozen.commits.base
                        && planned.base.tree == frozen.trees.base
                        && planned.candidate.commit == frozen.commits.candidate
                        && planned.candidate.tree == frozen.trees.candidate
                })
        })
}
