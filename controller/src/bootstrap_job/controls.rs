use amiss_wire::controls::{DebtSnapshot, OrganizationFloor, WaiverBundle};
use amiss_wire::digest::Digest;
use amiss_wire::json;
use amiss_wire::model::{BranchRef, RepositoryIdentity};
use amiss_wire::requests::{
    ControlsRequest, REQUEST_STREAM_BYTES, RequestTrust, SuppliedControl, SuppliedTime,
};

use crate::RunIdentity;

use super::BootstrapJobError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquiredControl {
    pub bytes: Vec<u8>,
    pub trust_source: RequestTrust,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyControls {
    pub organization_floor: Option<AcquiredControl>,
    pub debt_snapshot: Option<AcquiredControl>,
    pub waiver_bundle: Option<AcquiredControl>,
}

#[derive(Clone, Copy)]
pub(super) struct ControlIdentity {
    pub(super) digest: Digest,
    pub(super) trust_source: RequestTrust,
}

pub(super) struct PolicyIdentity {
    pub(super) organization_floor: Option<ControlIdentity>,
    pub(super) debt_snapshot: Option<ControlIdentity>,
    pub(super) waiver_bundle: Option<ControlIdentity>,
}

struct ControlBinding {
    digest: Digest,
    repository: RepositoryIdentity,
    ref_name: BranchRef,
    organization_floor_digest: Option<Digest>,
}

pub(super) fn identity(policy: &PolicyControls) -> Result<PolicyIdentity, BootstrapJobError> {
    Ok(PolicyIdentity {
        organization_floor: policy
            .organization_floor
            .as_ref()
            .map(|control| {
                control_identity(
                    control,
                    OrganizationFloor::parse,
                    |floor| floor.digest,
                    BootstrapJobError::OrganizationFloor,
                )
            })
            .transpose()?,
        debt_snapshot: policy
            .debt_snapshot
            .as_ref()
            .map(|control| {
                control_identity(
                    control,
                    DebtSnapshot::parse,
                    |snapshot| snapshot.digest,
                    BootstrapJobError::DebtSnapshot,
                )
            })
            .transpose()?,
        waiver_bundle: policy
            .waiver_bundle
            .as_ref()
            .map(|control| {
                control_identity(
                    control,
                    WaiverBundle::parse,
                    |bundle| bundle.digest,
                    BootstrapJobError::WaiverBundle,
                )
            })
            .transpose()?,
    })
}

fn control_identity<T, E>(
    control: &AcquiredControl,
    parse: impl FnOnce(&[u8]) -> Result<T, E>,
    digest: impl FnOnce(&T) -> Digest,
    error: BootstrapJobError,
) -> Result<ControlIdentity, BootstrapJobError> {
    within_stream_ceiling(control.bytes.len())
        .then_some(())
        .ok_or(error)?;
    parse(&control.bytes)
        .map(|value| ControlIdentity {
            digest: digest(&value),
            trust_source: control.trust_source,
        })
        .map_err(|_defect| error)
}

pub(super) fn request(
    policy: &PolicyControls,
    run: &RunIdentity,
    trusted_time: SuppliedTime,
    execution_constraint: SuppliedControl,
) -> Result<ControlsRequest, BootstrapJobError> {
    let organization_floor = policy
        .organization_floor
        .as_ref()
        .map(|control| {
            bound_control(
                control,
                run,
                None,
                |bytes| {
                    OrganizationFloor::parse(bytes).map(|floor| ControlBinding {
                        digest: floor.digest,
                        repository: floor.repository,
                        ref_name: floor.ref_name,
                        organization_floor_digest: None,
                    })
                },
                BootstrapJobError::OrganizationFloor,
            )
        })
        .transpose()?;
    let floor_digest = organization_floor
        .as_ref()
        .map(|control| control.expected_digest);
    let debt_snapshot = policy
        .debt_snapshot
        .as_ref()
        .map(|control| {
            bound_control(
                control,
                run,
                floor_digest,
                |bytes| {
                    DebtSnapshot::parse(bytes).map(|snapshot| ControlBinding {
                        digest: snapshot.digest,
                        repository: snapshot.repository,
                        ref_name: snapshot.ref_name,
                        organization_floor_digest: Some(snapshot.organization_floor_digest),
                    })
                },
                BootstrapJobError::DebtSnapshot,
            )
        })
        .transpose()?;
    let waiver_bundle = policy
        .waiver_bundle
        .as_ref()
        .map(|control| {
            bound_control(
                control,
                run,
                floor_digest,
                |bytes| {
                    WaiverBundle::parse(bytes).map(|bundle| ControlBinding {
                        digest: bundle.digest,
                        repository: bundle.repository,
                        ref_name: bundle.ref_name,
                        organization_floor_digest: Some(bundle.organization_floor_digest),
                    })
                },
                BootstrapJobError::WaiverBundle,
            )
        })
        .transpose()?;
    Ok(ControlsRequest {
        organization_floor,
        debt_snapshot,
        waiver_bundle,
        trusted_time: Some(trusted_time),
        execution_constraint: Some(execution_constraint),
    })
}

pub(super) fn canonical_request(request: &ControlsRequest) -> Result<Vec<u8>, BootstrapJobError> {
    request
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::RequestEncoding)
        .and_then(|bytes| {
            within_stream_ceiling(bytes.len())
                .then_some(bytes)
                .ok_or(BootstrapJobError::RequestEncoding)
        })
}

fn within_stream_ceiling(length: usize) -> bool {
    u64::try_from(length).is_ok_and(|length| length <= REQUEST_STREAM_BYTES)
}

fn bound_control<E>(
    control: &AcquiredControl,
    run: &RunIdentity,
    organization_floor_digest: Option<Digest>,
    parse: impl FnOnce(&[u8]) -> Result<ControlBinding, E>,
    error: BootstrapJobError,
) -> Result<SuppliedControl, BootstrapJobError> {
    let binding = parse(&control.bytes).map_err(|_defect| error)?;
    (binding.repository == run.change.repository
        && binding.ref_name == run.refs.target
        && binding.organization_floor_digest == organization_floor_digest)
        .then_some(())
        .ok_or(BootstrapJobError::ControlBinding)?;
    let value = json::parse(&control.bytes).map_err(|_defect| error)?;
    Ok(SuppliedControl {
        value,
        expected_digest: binding.digest,
        trust_source: control.trust_source,
    })
}
