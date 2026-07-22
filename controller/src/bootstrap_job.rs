mod controls;

use std::fmt;

use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, TrustedTimeInput, TrustedTimeStatement,
};
use amiss_wire::json::{self, Value};
use amiss_wire::model::UtcInstant;
use amiss_wire::requests::{
    EvaluationRequest, RequestStreams, RequestTrust, SnapshotRequest, SuppliedControl,
    SuppliedTime, commit_candidate_identity_digest,
};

use crate::RunRequest;

pub use controls::{AcquiredControl, PolicyControls};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapJobError {
    RunIdentity,
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
            Self::OrganizationFloor => "the organization floor is invalid",
            Self::DebtSnapshot => "the debt snapshot is invalid",
            Self::WaiverBundle => "the waiver bundle is invalid",
            Self::ControlBinding => "an external control names another run",
            Self::ExecutionConstraint => "the execution constraint is invalid",
            Self::TrustedTime => "the trusted time is invalid",
            Self::RequestEncoding => "the sealed requests cannot be encoded",
        })
    }
}

impl std::error::Error for BootstrapJobError {}

pub struct BootstrapJobInput<'a> {
    pub run: &'a RunRequest,
    pub profile: Profile,
    pub policy: &'a PolicyControls,
    pub execution: &'a ExecutionConstraintDescriptor,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapJob {
    streams: RequestStreams,
    constraint: Vec<u8>,
}

impl BootstrapJob {
    /// Joins one authenticated run to its exact canonical bootstrap inputs.
    /// The caller still owns independent repository and action acquisition.
    ///
    /// # Errors
    ///
    /// The run is internally inconsistent, a control is malformed or names
    /// another run, trusted time is invalid, or canonical encoding fails.
    pub fn new(input: BootstrapJobInput<'_>) -> Result<Self, BootstrapJobError> {
        let run = &input.run.run;
        if input.run.delivery.provider != run.change.provider
            || input.run.provider_run.object_format != run.object_format
            || input.run.provider_run.candidate_commit != run.commits.candidate
        {
            return Err(BootstrapJobError::RunIdentity);
        }

        let mut evaluation = EvaluationRequest::commit_pair(
            input.profile,
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

        let constraint = input
            .execution
            .canonical_bytes()
            .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
        let constraint_value =
            json::parse(&constraint).map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
        let controls = controls::request(
            input.policy,
            run,
            supplied_time(input.run, &statement, statement_value),
            SuppliedControl {
                value: constraint_value,
                expected_digest: input.execution.digest,
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
            controls: controls
                .canonical_bytes()
                .map_err(|_defect| BootstrapJobError::RequestEncoding)?,
        };
        Ok(Self {
            streams,
            constraint,
        })
    }

    pub const fn streams(&self) -> &RequestStreams {
        &self.streams
    }

    pub fn constraint(&self) -> &[u8] {
        &self.constraint
    }
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
