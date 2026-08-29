use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::digest::hj;
use amiss_wire::json::Value;

use super::controls;
use super::{
    BootstrapJobError, CheckBinding, CheckPlan, ExternalPolicy, PolicyControls,
    SemanticEvidenceExpectation, SemanticEvidenceTemplate, WorkflowArtifactExpectation,
};

const CHECK_PLAN_DOMAIN: &str = "amiss/controller-required-check-plan-v6";
const CANDIDATE_BINDING: &str = "provider-run-candidate-commit";

/// Freezes the controller-owned policy and required-check target reused by
/// every claim for one authenticated delivery.
///
/// # Errors
///
/// A policy artifact or execution constraint is invalid.
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
    let policy_identity = controls::identity(&policy)?;
    let constraint = execution
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    controls::validate_request_size(&policy, &policy_identity, &execution, &constraint)?;
    let digest = hj(
        CHECK_PLAN_DOMAIN,
        &plan_value(
            profile,
            policy.external_policy,
            &policy_identity,
            &policy.semantic_evidence,
            &policy.semantic_acquisitions,
            &policy.workflow_artifacts,
            &execution,
        ),
    );
    Ok(CheckPlan {
        digest,
        profile,
        policy,
        execution,
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
        required_status_name: plan.execution.required_status_name().to_owned(),
        execution_constraint_digest: plan.execution.digest(),
    }
}

pub(super) fn validated_plan(plan: &CheckPlan) -> Result<CheckPlan, BootstrapJobError> {
    let checked = check_plan(plan.profile, plan.policy.clone(), plan.execution.clone())?;
    (checked.digest == plan.digest)
        .then_some(checked)
        .ok_or(BootstrapJobError::CheckPlan)
}

fn plan_value(
    profile: Profile,
    external_policy: ExternalPolicy,
    policy: &controls::PolicyIdentity,
    semantic_evidence: &[SemanticEvidenceTemplate],
    semantic_acquisitions: &[SemanticEvidenceExpectation],
    workflow_artifacts: &[WorkflowArtifactExpectation],
    execution: &ExecutionConstraintDescriptor,
) -> Value {
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string(CHECK_PLAN_DOMAIN.to_owned()),
        ),
        (
            "profile".to_owned(),
            Value::string(profile.as_ref().to_owned()),
        ),
        (
            "external_policy".to_owned(),
            Value::string(external_policy.as_ref().to_owned()),
        ),
        (
            "organization_floor".to_owned(),
            control_identity_value(policy.organization_floor),
        ),
        (
            "debt_snapshot".to_owned(),
            control_identity_value(policy.debt_snapshot),
        ),
        (
            "waiver_bundle".to_owned(),
            control_identity_value(policy.waiver_bundle),
        ),
        (
            "execution_constraint_digest".to_owned(),
            Value::string(execution.digest().to_string()),
        ),
        (
            "required_status_name".to_owned(),
            Value::string(execution.required_status_name().to_owned()),
        ),
        (
            "semantic_evidence".to_owned(),
            Value::array(
                semantic_evidence
                    .iter()
                    .map(|template| {
                        Value::object(vec![
                            (
                                "producer_kind".to_owned(),
                                Value::string(template.producer_kind.as_str().to_owned()),
                            ),
                            (
                                "producer_identity".to_owned(),
                                Value::string(template.producer_identity.as_str().to_owned()),
                            ),
                            (
                                "producer_version".to_owned(),
                                Value::string(template.producer_version.clone()),
                            ),
                            (
                                "context_digest".to_owned(),
                                Value::string(template.context_digest.to_string()),
                            ),
                            (
                                "input_digest".to_owned(),
                                Value::string(template.input_digest.to_string()),
                            ),
                            ("complete".to_owned(), Value::Bool(template.complete)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "semantic_acquisitions".to_owned(),
            Value::array(
                semantic_acquisitions
                    .iter()
                    .map(expectation_value)
                    .collect(),
            ),
        ),
        (
            "workflow_artifacts".to_owned(),
            Value::array(
                workflow_artifacts
                    .iter()
                    .map(workflow_artifact_value)
                    .collect(),
            ),
        ),
    ])
}

fn expectation_value(expectation: &SemanticEvidenceExpectation) -> Value {
    Value::object(vec![
        (
            "acquisition_identity".to_owned(),
            Value::string(expectation.acquisition_identity.as_str().to_owned()),
        ),
        (
            "producer_kind".to_owned(),
            Value::string(expectation.producer_kind.as_str().to_owned()),
        ),
        (
            "producer_identity".to_owned(),
            Value::string(expectation.producer_identity.as_str().to_owned()),
        ),
        (
            "producer_version".to_owned(),
            Value::string(expectation.producer_version.clone()),
        ),
        (
            "context_digest".to_owned(),
            Value::string(expectation.context_digest.to_string()),
        ),
    ])
}

fn workflow_artifact_value(expectation: &WorkflowArtifactExpectation) -> Value {
    Value::object(vec![
        (
            "provider_namespace".to_owned(),
            Value::string(expectation.provider.namespace.as_str().to_owned()),
        ),
        (
            "provider_instance".to_owned(),
            Value::string(expectation.provider.instance.as_str().to_owned()),
        ),
        (
            "repository_host".to_owned(),
            Value::string(expectation.repository.host().to_owned()),
        ),
        (
            "repository_owner".to_owned(),
            Value::string(expectation.repository.owner().to_owned()),
        ),
        (
            "repository_name".to_owned(),
            Value::string(expectation.repository.name().to_owned()),
        ),
        (
            "workflow_identity".to_owned(),
            Value::string(expectation.workflow_identity.as_str().to_owned()),
        ),
        (
            "event".to_owned(),
            Value::string(expectation.event.as_str().to_owned()),
        ),
        (
            "artifact_name".to_owned(),
            Value::string(expectation.artifact_name.clone()),
        ),
        (
            "payload_file".to_owned(),
            Value::string(expectation.payload_file.as_str().to_owned()),
        ),
        (
            "archive_byte_limit".to_owned(),
            Value::Integer(i64::try_from(expectation.archive_byte_limit).unwrap_or(i64::MAX)),
        ),
        (
            "file_byte_limit".to_owned(),
            Value::Integer(i64::try_from(expectation.file_byte_limit).unwrap_or(i64::MAX)),
        ),
        (
            "candidate_binding".to_owned(),
            Value::string(CANDIDATE_BINDING.to_owned()),
        ),
        (
            "semantic".to_owned(),
            expectation_value(&expectation.semantic),
        ),
    ])
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

fn control_identity_value(identity: Option<controls::ControlIdentity>) -> Value {
    identity.map_or(Value::Null, |control| {
        Value::object(vec![
            (
                "digest".to_owned(),
                Value::string(control.digest.to_string()),
            ),
            (
                "trust_source".to_owned(),
                Value::string(control.trust_source.as_ref().to_owned()),
            ),
        ])
    })
}
