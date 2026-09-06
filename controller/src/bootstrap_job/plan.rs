use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, canonical_debt_snapshot,
    canonical_execution_constraint, canonical_organization_floor, canonical_waiver_bundle,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::requests::{REQUEST_STREAM_BYTES, SuppliedControl};

mod model;

use model::{ControlIdentity, PlanIdentity, SemanticIdentity, WorkflowArtifactIdentity};

use super::controls;
use super::{
    BootstrapJobError, CheckBinding, CheckPlan, PolicyControls, SemanticEvidenceExpectation,
    WorkflowArtifactExpectation,
};

const CHECK_PLAN_DOMAIN: &str = "amiss/controller-required-check-plan-v6";
const CANDIDATE_BINDING: &str = "provider-run-candidate-commit";

/// Freezes the controller-owned policy and required-check target reused by
/// every claim for one authenticated delivery.
///
/// # Errors
///
/// A policy artifact or execution constraint is invalid, or the identity cannot be encoded.
pub fn check_plan(
    profile: Profile,
    mut policy: PolicyControls,
    execution: ExecutionConstraintDescriptor,
) -> Result<CheckPlan, BootstrapJobError> {
    policy.semantic_acquisitions = normalized_expectations(&policy.semantic_acquisitions)?;
    policy.workflow_artifacts = normalized_workflow_artifacts(&policy.workflow_artifacts)?;
    let acquisition_expectations =
        normalized_expectations(&semantic_acquisition_expectations(&policy))?;
    if policy
        .semantic_evidence
        .len()
        .checked_add(acquisition_expectations.len())
        .is_none_or(|count| count > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT)
    {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let organization_floor = control_identity(
        policy.organization_floor.as_ref(),
        canonical_organization_floor,
        BootstrapJobError::OrganizationFloor,
    )?;
    let debt_snapshot = control_identity(
        policy.debt_snapshot.as_ref(),
        canonical_debt_snapshot,
        BootstrapJobError::DebtSnapshot,
    )?;
    let waiver_bundle = control_identity(
        policy.waiver_bundle.as_ref(),
        canonical_waiver_bundle,
        BootstrapJobError::WaiverBundle,
    )?;
    let (_, execution_digest) = canonical_execution_constraint(&execution)
        .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    controls::validate_request_size(&policy, execution_digest, &execution)?;
    let identity = PlanIdentity {
        debt_snapshot,
        execution_constraint_digest: execution_digest,
        external_policy: policy.external_policy,
        organization_floor,
        profile,
        required_status_name: &execution.required_status_name,
        schema: CHECK_PLAN_DOMAIN,
        semantic_acquisitions: &policy.semantic_acquisitions,
        semantic_evidence: policy
            .semantic_evidence
            .iter()
            .map(|template| SemanticIdentity {
                complete: template.complete,
                context_digest: template.producer.context_digest,
                input_digest: template.producer.input_digest,
                producer_identity: &template.producer.identity,
                producer_kind: template.producer.kind,
                producer_version: &template.producer.version,
            })
            .collect(),
        waiver_bundle,
        workflow_artifacts: policy
            .workflow_artifacts
            .iter()
            .map(|artifact| WorkflowArtifactIdentity {
                archive_byte_limit: artifact.archive_byte_limit,
                artifact_name: &artifact.artifact_name,
                candidate_binding: CANDIDATE_BINDING,
                event: artifact.event.as_str(),
                file_byte_limit: artifact.file_byte_limit,
                payload_file: &artifact.payload_file,
                provider_instance: artifact.provider.instance.as_str(),
                provider_namespace: artifact.provider.namespace.as_str(),
                repository_host: artifact.repository.host(),
                repository_name: artifact.repository.name(),
                repository_owner: artifact.repository.owner(),
                semantic: &artifact.semantic,
                workflow_identity: artifact.workflow_identity.as_str(),
            })
            .collect(),
    };
    let bytes = serde_json_canonicalizer::to_vec(&identity)
        .map_err(|_defect| BootstrapJobError::PlanEncoding)?;
    let digest = hb(CHECK_PLAN_DOMAIN, &bytes);
    Ok(CheckPlan {
        digest,
        profile,
        policy,
        execution,
        execution_digest,
    })
}

/// Projects the small retry-safe binding persisted by the delivery record.
///
/// # Errors
///
/// The public plan fields no longer reproduce the frozen digest.
pub fn check_binding(plan: &CheckPlan) -> Result<CheckBinding, BootstrapJobError> {
    validated_plan(plan).map(|checked| binding(&checked))
}

pub(super) fn binding(plan: &CheckPlan) -> CheckBinding {
    CheckBinding {
        plan_digest: plan.digest,
        required_status_name: plan.execution.required_status_name.clone(),
        execution_constraint_digest: plan.execution_digest,
    }
}

pub(super) fn validated_plan(plan: &CheckPlan) -> Result<CheckPlan, BootstrapJobError> {
    let checked = check_plan(plan.profile, plan.policy.clone(), plan.execution.clone())?;
    (checked.digest == plan.digest)
        .then_some(checked)
        .ok_or(BootstrapJobError::CheckPlan)
}

fn control_identity<T, E>(
    control: Option<&SuppliedControl<T>>,
    canonical: impl FnOnce(&T) -> Result<(Vec<u8>, Digest), E>,
    error: BootstrapJobError,
) -> Result<Option<ControlIdentity>, BootstrapJobError> {
    control
        .map(|control| {
            let (bytes, digest) = canonical(&control.value).map_err(|_defect| error)?;
            (digest == control.expected_digest
                && u64::try_from(bytes.len()).is_ok_and(|length| length <= REQUEST_STREAM_BYTES))
            .then_some(ControlIdentity {
                digest,
                trust_source: control.trust_source,
            })
            .ok_or(error)
        })
        .transpose()
}

pub(super) fn semantic_acquisition_expectations(
    policy: &PolicyControls,
) -> Vec<SemanticEvidenceExpectation> {
    let mut expectations = Vec::with_capacity(
        policy
            .semantic_acquisitions
            .len()
            .saturating_add(policy.workflow_artifacts.len()),
    );
    expectations.extend_from_slice(&policy.semantic_acquisitions);
    expectations.extend(
        policy
            .workflow_artifacts
            .iter()
            .map(|artifact| artifact.semantic.clone()),
    );
    expectations
}

fn normalized_workflow_artifacts(
    artifacts: &[WorkflowArtifactExpectation],
) -> Result<Vec<WorkflowArtifactExpectation>, BootstrapJobError> {
    if artifacts.iter().any(|artifact| {
        artifact.repository.host() != artifact.provider.instance.as_str()
            || artifact.artifact_name.is_empty()
            || artifact.artifact_name.len() > 256
            || artifact.artifact_name.chars().any(char::is_control)
            || artifact
                .payload_file
                .as_str()
                .bytes()
                .any(|byte| byte.is_ascii_control())
            || !(1..=super::MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES)
                .contains(&artifact.archive_byte_limit)
            || !(1..=super::MAX_WORKFLOW_ARTIFACT_FILE_BYTES).contains(&artifact.file_byte_limit)
    }) {
        return Err(BootstrapJobError::WorkflowArtifact);
    }
    let mut normalized = artifacts.to_vec();
    normalized.sort();
    if normalized.windows(2).any(|pair| {
        matches!(pair, [left, right]
            if left.provider == right.provider
                && left.repository == right.repository
                && left.workflow_identity == right.workflow_identity
                && left.event == right.event
                && left.artifact_name == right.artifact_name)
    }) {
        return Err(BootstrapJobError::WorkflowArtifact);
    }
    Ok(normalized)
}

pub(super) fn normalized_expectations(
    expectations: &[SemanticEvidenceExpectation],
) -> Result<Vec<SemanticEvidenceExpectation>, BootstrapJobError> {
    if expectations.iter().any(|expectation| {
        !amiss_wire::semantic::producer_version_valid(&expectation.producer_version)
    }) {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let mut normalized = expectations.to_vec();
    normalized.sort();
    if normalized.windows(2).any(|pair| {
        matches!(pair, [left, right] if left.acquisition_identity == right.acquisition_identity)
    })
    {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    Ok(normalized)
}
