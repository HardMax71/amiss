mod acquire;
mod effects;
mod floor;

use amiss_wire::controls::ResourceName;
use amiss_wire::digest::Digest;
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};
use amiss_wire::requests::RequestTrust;

pub use acquire::{Includes, PolicySide, acquire, acquire_entry};
pub use effects::{
    ControlSeed, DebtContext, Effects, InventoryState, TimeContext, WaiverContext, effects,
};
pub(crate) use floor::protected_control;
pub use floor::{
    PROTECTED_CONTROL_EVIDENCE_DOMAIN, ProtectedState, floor_inventory, floor_raises,
    protected_state, tightened_limits,
};

/// A verified organization floor as the wrapper supplies it: the parsed
/// value plus the external trust source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloorInput {
    pub floor: amiss_wire::controls::OrganizationFloor,
    pub trust_source: RequestTrust,
}

/// The floor's binding: its repository and full ref must equal the run's,
/// and the selected profile must be at least the floor minimum under
/// `observe < enforce-introduced < enforce`. Any violation is a
/// control-binding mismatch.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail.
pub fn verify_floor(
    input: &FloorInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    profile: amiss_wire::controls::Profile,
) -> Result<(), ErrorDetail> {
    let mismatch = ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        path_bytes: None,
        resource: None,
    };
    let Some(identity) = repository else {
        return Err(mismatch);
    };
    let floor = &input.floor;
    if floor.repository() != identity {
        return Err(mismatch);
    }
    if target_ref != Some(floor.ref_name().as_str()) {
        return Err(mismatch);
    }
    if profile < floor.minimum_profile() {
        return Err(mismatch);
    }
    Ok(())
}

/// A verified debt snapshot as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtInput {
    pub snapshot: amiss_wire::controls::DebtSnapshot,
    pub trust_source: RequestTrust,
}

/// A verified waiver bundle as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverInput {
    pub bundle: amiss_wire::controls::WaiverBundle,
    pub trust_source: RequestTrust,
}

/// The trusted-time statement plus the wrapper's provider-authenticated run
/// context the statement must identify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeInput {
    pub statement: amiss_wire::controls::TrustedTimeStatement,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
}

/// A verified execution constraint as the wrapper supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintInput {
    pub descriptor: amiss_wire::controls::ExecutionConstraintDescriptor,
    pub trust_source: RequestTrust,
}

const fn binding_mismatch_row() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::ControlBindingMismatch,
        path: None,
        path_bytes: None,
        resource: None,
    }
}

pub(crate) const fn trusted_time_invalid_row() -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::TrustedTimeInvalid,
        path: None,
        path_bytes: None,
        resource: None,
    }
}

fn identity_matches(
    control: &amiss_wire::model::RepositoryIdentity,
    control_ref: &amiss_wire::model::BranchRef,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
) -> bool {
    repository == Some(control) && target_ref == Some(control_ref.as_str())
}

fn verify_item_limit(
    resource: ResourceName,
    item_count: usize,
    limit: u64,
) -> Result<(), ErrorDetail> {
    if u64::try_from(item_count).unwrap_or(u64::MAX) <= limit {
        Ok(())
    } else {
        Err(ErrorDetail {
            code: AnalysisErrorCode::ResourceLimitExceeded,
            path: None,
            path_bytes: None,
            resource: Some((resource, limit, limit.saturating_add(1))),
        })
    }
}

/// Verifies the statement's shape, lifetime, repository, ref, candidate
/// identity, and provider run, then returns the evaluation context carrying
/// its canonical digest.
///
/// # Errors
///
/// One `TRUSTED_TIME_INVALID` detail.
pub fn verify_time(
    input: &TimeInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    candidate_identity: &Digest,
) -> Result<TimeContext, ErrorDetail> {
    let statement = &input.statement;
    let bound = identity_matches(
        &statement.repository,
        &statement.ref_name,
        repository,
        target_ref,
    ) && statement.candidate_identity_digest == *candidate_identity
        && statement.provider == input.provider
        && statement.provider_run_id == input.provider_run_id
        && statement.provider_run_attempt == input.provider_run_attempt;
    if !bound {
        return Err(trusted_time_invalid_row());
    }
    let digest = amiss_wire::controls::canonical_trusted_time(statement)
        .map_err(|_defect| trusted_time_invalid_row())?
        .1;
    Ok(TimeContext {
        statement: statement.clone(),
        digest,
    })
}

/// The snapshot-level debt binding: repository, ref, the verified floor's
/// digest, every owner on the floor's allow-list, causal time bounds against
/// the trusted instant, and the effective item ceiling.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail, or the `debt-items` crossing.
pub fn verify_debt(
    input: &DebtInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    floor: Option<&FloorInput>,
    instant: &amiss_wire::model::UtcInstant,
    item_limit: u64,
) -> Result<(), ErrorDetail> {
    let snapshot = &input.snapshot;
    verify_item_limit(ResourceName::DebtItems, snapshot.items().len(), item_limit)?;
    let floor = floor.ok_or(binding_mismatch_row())?;
    let bound = identity_matches(
        snapshot.repository(),
        snapshot.ref_name(),
        repository,
        target_ref,
    ) && snapshot.organization_floor_digest() == floor.floor.digest()
        && snapshot.created_at() <= instant
        && snapshot.items().iter().all(|item| {
            item.created_at <= *instant
                && floor.floor.authorized_debt_owners().contains(&item.owner)
        });
    if bound {
        Ok(())
    } else {
        Err(binding_mismatch_row())
    }
}

/// The bundle-level waiver binding: repository, ref, the verified floor's
/// digest, bundle creation not after the trusted instant, and the effective
/// item ceiling. Issuer, kind, owner distinction, activity, and body
/// agreement are selected-item semantics, not binding.
///
/// # Errors
///
/// One `CONTROL_BINDING_MISMATCH` detail, or the `waiver-items` crossing.
pub fn verify_waiver(
    input: &WaiverInput,
    repository: Option<&amiss_wire::model::RepositoryIdentity>,
    target_ref: Option<&str>,
    floor: Option<&FloorInput>,
    instant: &amiss_wire::model::UtcInstant,
    item_limit: u64,
) -> Result<(), ErrorDetail> {
    let bundle = &input.bundle;
    verify_item_limit(ResourceName::WaiverItems, bundle.items().len(), item_limit)?;
    let floor = floor.ok_or(binding_mismatch_row())?;
    let bound = identity_matches(
        bundle.repository(),
        bundle.ref_name(),
        repository,
        target_ref,
    ) && bundle.organization_floor_digest() == floor.floor.digest()
        && bundle.created_at() <= instant;
    if bound {
        Ok(())
    } else {
        Err(binding_mismatch_row())
    }
}
