mod controls;

use std::sync::Arc;

use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, TrustedTimeInput, TrustedTimeStatement,
};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{self, Value};
use amiss_wire::model::UtcInstant;
use amiss_wire::requests::{
    EvaluationRequest, RequestStreams, RequestTrust, SnapshotRequest, SuppliedControl,
    SuppliedTime, commit_candidate_identity_digest,
};

use crate::RunRequest;

pub use controls::{AcquiredControl, PolicyControls};

const CHECK_PLAN_DOMAIN: &str = "amiss/controller-required-check-plan-v3";

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Deserialize,
    serde::Serialize,
    strum::AsRefStr,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ExternalPolicy {
    Off,
    #[default]
    Advisory,
    BlockConfirmedRefutations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BootstrapJobError {
    #[error("the authenticated run identity is inconsistent")]
    RunIdentity,
    #[error("the check plan changed after validation")]
    CheckPlan,
    #[error("the organization floor is invalid")]
    OrganizationFloor,
    #[error("the debt snapshot is invalid")]
    DebtSnapshot,
    #[error("the waiver bundle is invalid")]
    WaiverBundle,
    #[error("an external control names another run")]
    ControlBinding,
    #[error("the execution constraint is invalid")]
    ExecutionConstraint,
    #[error("the trusted time is invalid")]
    TrustedTime,
    #[error("semantic evidence is invalid")]
    SemanticEvidence,
    #[error("the sealed requests cannot be encoded within the stream ceiling")]
    RequestEncoding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvidenceTemplate {
    pub(crate) producer_kind: amiss_wire::model::ArtifactId,
    pub(crate) producer_identity: amiss_wire::model::ArtifactId,
    pub(crate) producer_version: String,
    pub(crate) input_digest: Digest,
    pub(crate) complete: bool,
    pub(crate) observations: Arc<[Value]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckPlan {
    pub digest: Digest,
    pub profile: Profile,
    pub policy: PolicyControls,
    pub execution: ExecutionConstraintDescriptor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckBinding {
    pub plan_digest: Digest,
    pub required_status_name: String,
    pub execution_constraint_digest: Digest,
}

/// Freezes the controller-owned policy and required-check target reused by
/// every claim for one authenticated delivery.
///
/// # Errors
///
/// A policy artifact or execution constraint is invalid.
pub fn check_plan(
    profile: Profile,
    policy: PolicyControls,
    execution: ExecutionConstraintDescriptor,
) -> Result<CheckPlan, BootstrapJobError> {
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

pub struct BootstrapJobInput<'a> {
    pub run: &'a RunRequest,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapJob {
    pub streams: RequestStreams,
    pub constraint: Vec<u8>,
}

/// Joins one authenticated run to its exact canonical bootstrap inputs. The
/// caller still owns independent repository and action acquisition.
///
/// # Errors
///
/// The run is internally inconsistent, a control is malformed or names
/// another run, trusted time is invalid, or a canonical request is invalid or
/// exceeds its stream ceiling.
pub fn bootstrap_job(input: BootstrapJobInput<'_>) -> Result<BootstrapJob, BootstrapJobError> {
    let checked_plan = validated_plan(&input.run.plan)?;
    (binding(&checked_plan) == input.run.check)
        .then_some(())
        .ok_or(BootstrapJobError::CheckPlan)?;
    let run = &input.run.run;
    (input.run.delivery.provider == run.change.provider
        && input.run.provider_run.object_format == run.object_format
        && input.run.provider_run.candidate_commit == run.commits.candidate)
        .then_some(())
        .ok_or(BootstrapJobError::RunIdentity)?;

    let mut evaluation = EvaluationRequest::commit_pair(
        checked_plan.profile,
        run.object_format,
        run.commits.base.clone(),
        run.commits.candidate.clone(),
    );
    evaluation.repository = Some(run.change.repository.clone());
    evaluation.forge = Some(run.refs.forge);
    evaluation.candidate_ref = Some(run.refs.candidate.clone());
    evaluation.target_ref = Some(run.refs.target.clone());
    evaluation.default_branch_ref = Some(run.refs.default_branch.clone());
    let candidate_identity =
        commit_candidate_identity_digest(&evaluation, &run.trees.base, &run.trees.candidate)
            .ok_or(BootstrapJobError::RunIdentity)?;

    let statement = TrustedTimeStatement::new(TrustedTimeInput {
        repository: run.change.repository.clone(),
        ref_name: run.refs.target.clone(),
        candidate_identity_digest: candidate_identity,
        provider: input.run.delivery.provider.namespace.as_str().to_owned(),
        provider_run_id: input.run.provider_run.run_id.as_str().to_owned(),
        provider_run_attempt: input.run.provider_run.attempt.get(),
        evaluation_instant: input.evaluation_instant,
        valid_until: input.valid_until,
    })
    .map_err(|_defect| BootstrapJobError::TrustedTime)?;
    let statement_bytes = statement
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::TrustedTime)?;
    let statement_value =
        json::parse(&statement_bytes).map_err(|_defect| BootstrapJobError::TrustedTime)?;

    let constraint = checked_plan
        .execution
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    let constraint_value =
        json::parse(&constraint).map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    let controls = controls::request(
        &checked_plan.policy,
        run,
        supplied_time(input.run, &statement, statement_value),
        SuppliedControl {
            value: constraint_value,
            expected_digest: checked_plan.execution.digest(),
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
        bind_semantic_evidence(&checked_plan.policy.semantic_evidence, candidate_identity)?,
    )?;
    let streams = RequestStreams {
        evaluation: evaluation
            .canonical_bytes()
            .map_err(|_defect| BootstrapJobError::RequestEncoding)?,
        snapshot: SnapshotRequest::git_objects()
            .canonical_bytes()
            .map_err(|_defect| BootstrapJobError::RequestEncoding)?,
        controls: controls::canonical_request(&controls)?,
    };
    Ok(BootstrapJob {
        streams,
        constraint,
    })
}

/// Binds controller-produced evidence to one exact candidate and orders the
/// resulting envelope set by payload identity.
///
/// # Errors
///
/// A template cannot form a valid bounded envelope or two envelopes collide.
pub fn bind_semantic_evidence(
    templates: &[SemanticEvidenceTemplate],
    candidate_identity_digest: Digest,
) -> Result<Vec<Value>, BootstrapJobError> {
    if templates.len() > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let mut envelopes = templates
        .iter()
        .map(|template| {
            let value = amiss_wire::semantic::envelope(amiss_wire::semantic::SemanticEvidence {
                candidate_identity_digest,
                source_report_payload_digest: None,
                producer_kind: template.producer_kind.clone(),
                producer_identity: template.producer_identity.clone(),
                producer_version: template.producer_version.clone(),
                input_digest: template.input_digest,
                complete: template.complete,
                observations: template.observations.as_ref().to_vec(),
            })
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
            let payload_digest = value
                .text("payload_digest")
                .and_then(Digest::from_wire)
                .ok_or(BootstrapJobError::SemanticEvidence)?;
            Ok((payload_digest, value))
        })
        .collect::<Result<Vec<_>, BootstrapJobError>>()?;
    envelopes.sort_by_key(|(digest, _value)| *digest);
    if envelopes
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.0 == right.0))
    {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    Ok(envelopes
        .into_iter()
        .map(|(_digest, value)| value)
        .collect())
}

fn binding(plan: &CheckPlan) -> CheckBinding {
    CheckBinding {
        plan_digest: plan.digest,
        required_status_name: plan.execution.required_status_name().to_owned(),
        execution_constraint_digest: plan.execution.digest(),
    }
}

fn validated_plan(plan: &CheckPlan) -> Result<CheckPlan, BootstrapJobError> {
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
    execution: &ExecutionConstraintDescriptor,
) -> Value {
    Value::object(vec![
        (
            "schema".to_owned(),
            Value::string(CHECK_PLAN_DOMAIN.to_owned()),
        ),
        (
            "profile".to_owned(),
            Value::string(
                match profile {
                    Profile::Observe => "observe",
                    Profile::EnforceIntroduced => "enforce-introduced",
                    Profile::Enforce => "enforce",
                }
                .to_owned(),
            ),
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
                                "input_digest".to_owned(),
                                Value::string(template.input_digest.to_string()),
                            ),
                            ("complete".to_owned(), Value::Bool(template.complete)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
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

fn supplied_time(run: &RunRequest, statement: &TrustedTimeStatement, value: Value) -> SuppliedTime {
    SuppliedTime {
        value,
        expected_digest: statement.digest(),
        provider: run.delivery.provider.namespace.as_str().to_owned(),
        provider_run_id: run.provider_run.run_id.as_str().to_owned(),
        provider_run_attempt: run.provider_run.attempt.get(),
    }
}
