mod controls;
mod plan;
mod semantic;

use std::sync::Arc;

use amiss_wire::controls::{
    ExecutionConstraintDescriptor, Profile, TrustedTimeController, TrustedTimeSchema,
    TrustedTimeStatement, canonical_execution_constraint, canonical_trusted_time,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, RepoPathText, RepositoryIdentity, UtcInstant};
use amiss_wire::requests::{
    EvaluationRequest, RequestStreams, RequestTrust, SnapshotRequest, SuppliedControl,
    SuppliedSemanticEvidence, SuppliedTime, commit_candidate_identity_digest,
};

use crate::{OpaqueId, ProviderIdentity, RunRequest};

pub use amiss_wire::semantic::SemanticEvidenceTemplate;
pub use controls::PolicyControls;
pub use plan::{check_binding, check_plan};
pub use semantic::bind_semantic_evidence;

use plan::{binding, validated_plan};

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
    #[error("the check plan identity cannot be encoded")]
    PlanEncoding,
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
    #[error("a workflow artifact expectation is invalid")]
    WorkflowArtifact,
    #[error("the sealed requests cannot be encoded within the stream ceiling")]
    RequestEncoding,
}

pub const SEMANTIC_INPUT_ARTIFACT_BYTES: u64 = 50_331_648;
pub const MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES: u64 = 33_554_432;
pub const MAX_WORKFLOW_ARTIFACT_FILE_BYTES: u64 = amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquiredSemanticTemplate {
    pub acquisition_identity: ArtifactId,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundSemanticEvidence {
    pub supplied: Vec<SuppliedSemanticEvidence>,
    pub artifact: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct SemanticEvidenceExpectation {
    pub acquisition_identity: ArtifactId,
    pub producer_kind: amiss_wire::semantic::SemanticProducerKind,
    pub producer_identity: ArtifactId,
    pub producer_version: String,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkflowArtifactExpectation {
    pub provider: ProviderIdentity,
    pub repository: RepositoryIdentity,
    pub workflow_identity: OpaqueId,
    pub event: OpaqueId,
    pub artifact_name: String,
    pub payload_file: RepoPathText,
    pub archive_byte_limit: u64,
    pub file_byte_limit: u64,
    pub semantic: SemanticEvidenceExpectation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckPlan {
    pub digest: Digest,
    pub profile: Profile,
    pub policy: PolicyControls,
    pub execution: ExecutionConstraintDescriptor,
    pub execution_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckBinding {
    pub plan_digest: Digest,
    pub required_status_name: String,
    pub execution_constraint_digest: Digest,
}

pub struct BootstrapJobInput<'a> {
    pub run: &'a RunRequest,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
    pub acquired_semantic_templates: &'a [AcquiredSemanticTemplate],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapJob {
    pub streams: RequestStreams,
    pub constraint: Vec<u8>,
    pub semantic_artifact: Option<Vec<u8>>,
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

    let statement = TrustedTimeStatement {
        schema: TrustedTimeSchema::Current,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        repository: run.change.repository.clone(),
        ref_name: run.refs.target.clone(),
        candidate_identity_digest: candidate_identity,
        provider: input.run.delivery.provider.namespace.as_str().to_owned(),
        provider_run_id: input.run.provider_run.run_id.as_str().to_owned(),
        provider_run_attempt: input.run.provider_run.attempt.get(),
        evaluation_instant: input.evaluation_instant,
        valid_until: input.valid_until,
    };
    let (_, statement_digest) =
        canonical_trusted_time(&statement).map_err(|_defect| BootstrapJobError::TrustedTime)?;

    let (constraint, constraint_digest) =
        canonical_execution_constraint(&checked_plan.execution)
            .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?;
    let semantic_expectations = plan::semantic_acquisition_expectations(&checked_plan.policy);
    let semantic = bind_semantic_evidence(
        &checked_plan.policy.semantic_evidence,
        &semantic_expectations,
        input.acquired_semantic_templates,
        candidate_identity,
    )?;
    let controls = controls::request(
        checked_plan.policy,
        run,
        SuppliedTime {
            value: statement,
            expected_digest: statement_digest,
            provider: input.run.delivery.provider.namespace.as_str().to_owned(),
            provider_run_id: input.run.provider_run.run_id.as_str().to_owned(),
            provider_run_attempt: input.run.provider_run.attempt.get(),
        },
        SuppliedControl {
            value: checked_plan.execution.clone(),
            expected_digest: constraint_digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
        semantic.supplied,
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
        semantic_artifact: semantic.artifact,
    })
}
