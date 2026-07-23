use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor, TrustedTimeInput,
    TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::digest::Digest;
use amiss_wire::json;
use amiss_wire::model::{BranchRef, RepositoryIdentity, UtcInstant};
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

type DigestParser = fn(&[u8]) -> Option<Digest>;
type IdentityInput<'a> = (Option<&'a AcquiredControl>, DigestParser, BootstrapJobError);

pub(super) fn identity(policy: &PolicyControls) -> Result<PolicyIdentity, BootstrapJobError> {
    identity_values(policy).map(make_policy_identity)
}

fn identity_values(
    policy: &PolicyControls,
) -> Result<[Option<ControlIdentity>; 3], BootstrapJobError> {
    let [organization_floor, debt_snapshot, waiver_bundle] =
        identity_inputs(policy).map(|(control, digest, error)| {
            control
                .map(|control| {
                    verified_digest(control, digest)
                        .map(|digest| make_control_identity(control, digest))
                        .ok_or(error)
                })
                .transpose()
        });
    Ok([organization_floor?, debt_snapshot?, waiver_bundle?])
}

fn identity_inputs(policy: &PolicyControls) -> [IdentityInput<'_>; 3] {
    [
        (
            policy.organization_floor.as_ref(),
            |bytes| {
                OrganizationFloor::parse(bytes)
                    .ok()
                    .map(|floor| floor.digest)
            },
            BootstrapJobError::OrganizationFloor,
        ),
        (
            policy.debt_snapshot.as_ref(),
            |bytes| {
                DebtSnapshot::parse(bytes)
                    .ok()
                    .map(|snapshot| snapshot.digest)
            },
            BootstrapJobError::DebtSnapshot,
        ),
        (
            policy.waiver_bundle.as_ref(),
            |bytes| WaiverBundle::parse(bytes).ok().map(|bundle| bundle.digest),
            BootstrapJobError::WaiverBundle,
        ),
    ]
}

fn make_policy_identity(
    [organization_floor, debt_snapshot, waiver_bundle]: [Option<ControlIdentity>; 3],
) -> PolicyIdentity {
    PolicyIdentity {
        organization_floor,
        debt_snapshot,
        waiver_bundle,
    }
}

pub(super) fn validate_request_size(
    policy: &PolicyControls,
    identity: &PolicyIdentity,
    execution: &ExecutionConstraintDescriptor,
    execution_bytes: &[u8],
) -> Result<(), BootstrapJobError> {
    let request = ControlsRequest {
        organization_floor: plan_control(
            policy.organization_floor.as_ref(),
            identity.organization_floor,
            BootstrapJobError::OrganizationFloor,
        )?,
        debt_snapshot: plan_control(
            policy.debt_snapshot.as_ref(),
            identity.debt_snapshot,
            BootstrapJobError::DebtSnapshot,
        )?,
        waiver_bundle: plan_control(
            policy.waiver_bundle.as_ref(),
            identity.waiver_bundle,
            BootstrapJobError::WaiverBundle,
        )?,
        trusted_time: Some(maximal_trusted_time(execution.digest)?),
        execution_constraint: Some(SuppliedControl {
            value: json::parse(execution_bytes)
                .map_err(|_defect| BootstrapJobError::ExecutionConstraint)?,
            expected_digest: execution.digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        }),
    };
    canonical_request(&request).map(|_bytes| ())
}

fn plan_control(
    control: Option<&AcquiredControl>,
    identity: Option<ControlIdentity>,
    error: BootstrapJobError,
) -> Result<Option<SuppliedControl>, BootstrapJobError> {
    (control.is_some() == identity.is_some())
        .then_some(control.zip(identity))
        .ok_or(error)?
        .map(|(control, identity)| {
            json::parse(&control.bytes).map(|value| SuppliedControl {
                value,
                expected_digest: identity.digest,
                trust_source: identity.trust_source,
            })
        })
        .transpose()
        .map_err(|_defect| error)
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
    let statement = TrustedTimeStatement::new(TrustedTimeInput {
        repository,
        ref_name,
        candidate_identity_digest,
        provider: provider.clone(),
        provider_run_id: provider_run_id.clone(),
        provider_run_attempt: 9_007_199_254_740_991,
        evaluation_instant,
        valid_until,
    })
    .map_err(|_defect| BootstrapJobError::RequestEncoding)?;
    let value = statement
        .canonical_bytes()
        .map_err(|_defect| BootstrapJobError::RequestEncoding)
        .and_then(|bytes| {
            json::parse(&bytes).map_err(|_defect| BootstrapJobError::RequestEncoding)
        })?;
    Ok(SuppliedTime {
        value,
        expected_digest: statement.digest,
        provider,
        provider_run_id,
        provider_run_attempt: 9_007_199_254_740_991,
    })
}

fn verified_digest(control: &AcquiredControl, parser: DigestParser) -> Option<Digest> {
    within_stream_ceiling(control.bytes.len())
        .then(|| parser(&control.bytes))
        .flatten()
}

fn make_control_identity(control: &AcquiredControl, digest: Digest) -> ControlIdentity {
    ControlIdentity {
        digest,
        trust_source: control.trust_source,
    }
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
