mod controls;

use std::fmt;

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

const CHECK_PLAN_DOMAIN: &str = "amiss/controller-required-check-plan-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapJobError {
    RunIdentity,
    CheckPlan,
    OrganizationFloor,
    DebtSnapshot,
    WaiverBundle,
    ControlBinding,
    ExecutionConstraint,
    TrustedTime,
    RequestEncoding,
}

impl fmt::Display for BootstrapJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RunIdentity => "the authenticated run identity is inconsistent",
            Self::CheckPlan => "the check plan changed after validation",
            Self::OrganizationFloor => "the organization floor is invalid",
            Self::DebtSnapshot => "the debt snapshot is invalid",
            Self::WaiverBundle => "the waiver bundle is invalid",
            Self::ControlBinding => "an external control names another run",
            Self::ExecutionConstraint => "the execution constraint is invalid",
            Self::TrustedTime => "the trusted time is invalid",
            Self::RequestEncoding => {
                "the sealed requests cannot be encoded within the stream ceiling"
            }
        })
    }
}

impl std::error::Error for BootstrapJobError {}

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
    let _constraint = execution
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    let digest = hj(
        CHECK_PLAN_DOMAIN,
        &plan_value(profile, &policy_identity, &execution),
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
            expected_digest: checked_plan.execution.digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
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

fn binding(plan: &CheckPlan) -> CheckBinding {
    CheckBinding {
        plan_digest: plan.digest,
        required_status_name: plan.execution.required_status_name.clone(),
        execution_constraint_digest: plan.execution.digest,
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
    policy: &controls::PolicyIdentity,
    execution: &ExecutionConstraintDescriptor,
) -> Value {
    Value::Object(vec![
        (
            "schema".to_owned(),
            Value::String(CHECK_PLAN_DOMAIN.to_owned()),
        ),
        (
            "profile".to_owned(),
            Value::String(
                match profile {
                    Profile::Observe => "observe",
                    Profile::Enforce => "enforce",
                }
                .to_owned(),
            ),
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
            Value::String(execution.digest.to_string()),
        ),
        (
            "required_status_name".to_owned(),
            Value::String(execution.required_status_name.clone()),
        ),
    ])
}

fn control_identity_value(identity: Option<controls::ControlIdentity>) -> Value {
    identity.map_or(Value::Null, |control| {
        Value::Object(vec![
            (
                "digest".to_owned(),
                Value::String(control.digest.to_string()),
            ),
            (
                "trust_source".to_owned(),
                Value::String(control.trust_source.as_str().to_owned()),
            ),
        ])
    })
}

fn supplied_time(run: &RunRequest, statement: &TrustedTimeStatement, value: Value) -> SuppliedTime {
    SuppliedTime {
        value,
        expected_digest: statement.digest,
        provider: run.delivery.provider.namespace.as_str().to_owned(),
        provider_run_id: run.provider_run.run_id.as_str().to_owned(),
        provider_run_attempt: run.provider_run.attempt.get(),
    }
}
