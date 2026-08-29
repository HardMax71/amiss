use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::digest::hj;
use amiss_wire::json::Value;

use super::controls;
use super::{
    BootstrapJobError, CheckBinding, CheckPlan, ExternalPolicy, PolicyControls,
    SemanticEvidenceExpectation, SemanticEvidenceTemplate,
};

const CHECK_PLAN_DOMAIN: &str = "amiss/controller-required-check-plan-v5";

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
    if policy
        .semantic_evidence
        .len()
        .checked_add(policy.semantic_acquisitions.len())
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
