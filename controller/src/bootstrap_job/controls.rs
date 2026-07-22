use amiss_wire::controls::{DebtSnapshot, OrganizationFloor, WaiverBundle};
use amiss_wire::json;
use amiss_wire::requests::{
    ControlsRequest, REQUEST_STREAM_BYTES, RequestTrust, SuppliedControl, SuppliedTime,
};

use crate::RunIdentity;

use super::BootstrapJobError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquiredControl {
    bytes: Vec<u8>,
    trust_source: RequestTrust,
}

impl AcquiredControl {
    #[must_use]
    pub fn new(bytes: Vec<u8>, trust_source: RequestTrust) -> Option<Self> {
        (u64::try_from(bytes.len()).ok()? <= REQUEST_STREAM_BYTES).then_some(Self {
            bytes,
            trust_source,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyControls {
    pub organization_floor: Option<AcquiredControl>,
    pub debt_snapshot: Option<AcquiredControl>,
    pub waiver_bundle: Option<AcquiredControl>,
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
        .map(|control| floor(control, run))
        .transpose()?;
    let floor_digest = organization_floor
        .as_ref()
        .map(|control| control.expected_digest);
    let debt_snapshot = policy
        .debt_snapshot
        .as_ref()
        .map(|control| debt(control, run, floor_digest))
        .transpose()?;
    let waiver_bundle = policy
        .waiver_bundle
        .as_ref()
        .map(|control| waiver(control, run, floor_digest))
        .transpose()?;
    Ok(ControlsRequest {
        organization_floor,
        debt_snapshot,
        waiver_bundle,
        trusted_time: Some(trusted_time),
        execution_constraint: Some(execution_constraint),
    })
}

fn floor(
    control: &AcquiredControl,
    run: &RunIdentity,
) -> Result<SuppliedControl, BootstrapJobError> {
    let floor = OrganizationFloor::parse(&control.bytes)
        .map_err(|_defect| BootstrapJobError::OrganizationFloor)?;
    if floor.repository != run.change.repository || floor.ref_name != run.refs.target {
        return Err(BootstrapJobError::ControlBinding);
    }
    supplied(control, floor.digest, BootstrapJobError::OrganizationFloor)
}

fn debt(
    control: &AcquiredControl,
    run: &RunIdentity,
    floor_digest: Option<amiss_wire::digest::Digest>,
) -> Result<SuppliedControl, BootstrapJobError> {
    let snapshot =
        DebtSnapshot::parse(&control.bytes).map_err(|_defect| BootstrapJobError::DebtSnapshot)?;
    if snapshot.repository != run.change.repository
        || snapshot.ref_name != run.refs.target
        || Some(snapshot.organization_floor_digest) != floor_digest
    {
        return Err(BootstrapJobError::ControlBinding);
    }
    supplied(control, snapshot.digest, BootstrapJobError::DebtSnapshot)
}

fn waiver(
    control: &AcquiredControl,
    run: &RunIdentity,
    floor_digest: Option<amiss_wire::digest::Digest>,
) -> Result<SuppliedControl, BootstrapJobError> {
    let bundle =
        WaiverBundle::parse(&control.bytes).map_err(|_defect| BootstrapJobError::WaiverBundle)?;
    if bundle.repository != run.change.repository
        || bundle.ref_name != run.refs.target
        || Some(bundle.organization_floor_digest) != floor_digest
    {
        return Err(BootstrapJobError::ControlBinding);
    }
    supplied(control, bundle.digest, BootstrapJobError::WaiverBundle)
}

fn supplied(
    control: &AcquiredControl,
    digest: amiss_wire::digest::Digest,
    error: BootstrapJobError,
) -> Result<SuppliedControl, BootstrapJobError> {
    let value = json::parse(&control.bytes).map_err(|_defect| error)?;
    Ok(SuppliedControl {
        value,
        expected_digest: digest,
        trust_source: control.trust_source,
    })
}
