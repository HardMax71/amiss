use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor, TrustedTimeController,
    TrustedTimeSchema, TrustedTimeStatement, WaiverBundle, canonical_trusted_time,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{BranchRef, RepositoryIdentity, UtcInstant};
use amiss_wire::requests::{
    ControlsRequest, ControlsRequestSchema, REQUEST_STREAM_BYTES, RequestTrust, SuppliedControl,
    SuppliedTime,
};

use crate::RunIdentity;

use super::{BootstrapJobError, ExternalPolicy};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyControls {
    pub external_policy: ExternalPolicy,
    pub organization_floor: Option<SuppliedControl<OrganizationFloor>>,
    pub debt_snapshot: Option<SuppliedControl<DebtSnapshot>>,
    pub waiver_bundle: Option<SuppliedControl<WaiverBundle>>,
    pub semantic_evidence: Vec<super::SemanticEvidenceTemplate<'static>>,
    pub semantic_acquisitions: Vec<super::SemanticEvidenceExpectation>,
    pub workflow_artifacts: Vec<super::WorkflowArtifactExpectation>,
}

pub(super) fn validate_request_size(
    policy: &PolicyControls,
    execution_digest: Digest,
    execution: &ExecutionConstraintDescriptor,
) -> Result<(), BootstrapJobError> {
    let request = ControlsRequest {
        schema: ControlsRequestSchema::Current,
        organization_floor: policy.organization_floor.clone(),
        debt_snapshot: policy.debt_snapshot.clone(),
        waiver_bundle: policy.waiver_bundle.clone(),
        trusted_time: Some(maximal_trusted_time(execution_digest)?),
        execution_constraint: Some(SuppliedControl {
            value: execution.clone(),
            expected_digest: execution_digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        }),
        semantic_evidence: super::bind_semantic_evidence(
            &policy.semantic_evidence,
            &[],
            &[],
            execution_digest,
        )?
        .supplied,
    };
    canonical_request(&request).map(|_bytes| ())
}

fn maximal_trusted_time(
    candidate_identity_digest: Digest,
) -> Result<SuppliedTime, BootstrapJobError> {
    let provider = "a".repeat(128);
    let provider_run_id = "a".repeat(128);
    let repository = RepositoryIdentity::new(
        "\0".repeat(255),
        format!("{}/{}/{}", "a".repeat(85), "a".repeat(84), "a".repeat(84)),
        "a".repeat(100),
    )
    .ok_or(BootstrapJobError::RequestEncoding)?;
    let ref_name = BranchRef::new(format!("refs/heads/{}", "\"".repeat(255)))
        .ok_or(BootstrapJobError::RequestEncoding)?;
    let evaluation_instant = UtcInstant::new("9999-12-31T23:50:00Z".to_owned())
        .ok_or(BootstrapJobError::RequestEncoding)?;
    let valid_until = UtcInstant::new("9999-12-31T23:59:00Z".to_owned())
        .ok_or(BootstrapJobError::RequestEncoding)?;
    let statement = TrustedTimeStatement {
        schema: TrustedTimeSchema::Current,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        repository,
        ref_name,
        candidate_identity_digest,
        provider: provider.clone(),
        provider_run_id: provider_run_id.clone(),
        provider_run_attempt: 9_007_199_254_740_991,
        evaluation_instant,
        valid_until,
    };
    let (_, expected_digest) =
        canonical_trusted_time(&statement).map_err(|_defect| BootstrapJobError::RequestEncoding)?;
    Ok(SuppliedTime {
        value: statement,
        expected_digest,
        provider,
        provider_run_id,
        provider_run_attempt: 9_007_199_254_740_991,
    })
}

pub(super) fn request(
    policy: PolicyControls,
    run: &RunIdentity,
    trusted_time: SuppliedTime,
    execution_constraint: SuppliedControl<ExecutionConstraintDescriptor>,
    semantic_evidence: Vec<amiss_wire::requests::SuppliedSemanticEvidence>,
) -> Result<ControlsRequest, BootstrapJobError> {
    let floor_digest = policy
        .organization_floor
        .as_ref()
        .map(|control| control.expected_digest);
    let bindings = [
        policy
            .organization_floor
            .as_ref()
            .map(|control| (&control.value.repository, &control.value.ref_name, None)),
        policy.debt_snapshot.as_ref().map(|control| {
            (
                &control.value.repository,
                &control.value.ref_name,
                Some(control.value.organization_floor_digest),
            )
        }),
        policy.waiver_bundle.as_ref().map(|control| {
            (
                &control.value.repository,
                &control.value.ref_name,
                Some(control.value.organization_floor_digest),
            )
        }),
    ];
    if bindings
        .into_iter()
        .flatten()
        .any(|(repository, ref_name, required_floor)| {
            repository != &run.change.repository
                || ref_name != &run.refs.target
                || required_floor.is_some_and(|digest| Some(digest) != floor_digest)
        })
    {
        return Err(BootstrapJobError::ControlBinding);
    }
    Ok(ControlsRequest {
        schema: ControlsRequestSchema::Current,
        organization_floor: policy.organization_floor,
        debt_snapshot: policy.debt_snapshot,
        waiver_bundle: policy.waiver_bundle,
        trusted_time: Some(trusted_time),
        execution_constraint: Some(execution_constraint),
        semantic_evidence,
    })
}

pub(super) fn canonical_request(request: &ControlsRequest) -> Result<Vec<u8>, BootstrapJobError> {
    request
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::RequestEncoding)
        .and_then(|bytes| {
            u64::try_from(bytes.len())
                .is_ok_and(|length| length <= REQUEST_STREAM_BYTES)
                .then_some(bytes)
                .ok_or(BootstrapJobError::RequestEncoding)
        })
}
