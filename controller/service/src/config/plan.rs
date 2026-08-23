use std::path::{Path, PathBuf};

use amiss_controller::{
    AcquiredControl, CheckPlan, ExternalPolicy, INTERSPHINX_INVENTORY_BYTES, IntersphinxInventory,
    PolicyControls, check_plan, intersphinx_evidence,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::requests::{REQUEST_STREAM_BYTES, RequestTrust};
use serde::Deserialize;

use super::{ConfigError, read_regular};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPlanFiles {
    profile: String,
    #[serde(default)]
    external_policy: ExternalPolicy,
    execution_constraint_file: PathBuf,
    organization_floor_file: Option<PathBuf>,
    debt_snapshot_file: Option<PathBuf>,
    waiver_bundle_file: Option<PathBuf>,
    #[serde(default)]
    intersphinx_inventories: Vec<IntersphinxInventoryFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntersphinxInventoryFile {
    identity: String,
    base_url: String,
    file: PathBuf,
}

/// Loads and binds every trust input named by one service plan.
///
/// # Errors
///
/// A profile, trust file, execution constraint, or resulting plan is invalid.
pub fn load_plan(raw: &CheckPlanFiles) -> Result<CheckPlan, ConfigError> {
    let profile = match raw.profile.as_str() {
        "observe" => Profile::Observe,
        "enforce" => Profile::Enforce,
        _ => return Err(ConfigError::invalid("profile must be observe or enforce")),
    };
    let execution_bytes = read_regular(&raw.execution_constraint_file, REQUEST_STREAM_BYTES)?;
    let execution = ExecutionConstraintDescriptor::parse(&execution_bytes)
        .map_err(|defect| ConfigError::caused_by("execution constraint is invalid", defect))?;
    let semantic_evidence = intersphinx_evidence(load_intersphinx(&raw.intersphinx_inventories)?)
        .map_err(|defect| {
        ConfigError::caused_by("Intersphinx inventory configuration is invalid", defect)
    })?;
    let policy = PolicyControls {
        external_policy: raw.external_policy,
        organization_floor: load_control(raw.organization_floor_file.as_deref())?,
        debt_snapshot: load_control(raw.debt_snapshot_file.as_deref())?,
        waiver_bundle: load_control(raw.waiver_bundle_file.as_deref())?,
        semantic_evidence,
    };
    check_plan(profile, policy, execution)
        .map_err(|defect| ConfigError::caused_by("check plan is invalid", defect))
}

fn load_intersphinx(
    inventories: &[IntersphinxInventoryFile],
) -> Result<Vec<IntersphinxInventory>, ConfigError> {
    inventories
        .iter()
        .try_fold(
            (
                Vec::with_capacity(inventories.len()),
                INTERSPHINX_INVENTORY_BYTES,
            ),
            |(mut loaded, remaining), inventory| {
                let bytes = read_regular(&inventory.file, remaining)?;
                let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                let remaining = remaining.checked_sub(length).ok_or(ConfigError::invalid(
                    "Intersphinx inventory files exceed their byte ceiling",
                ))?;
                loaded.push(IntersphinxInventory {
                    identity: inventory.identity.clone(),
                    base_url: inventory.base_url.clone(),
                    bytes,
                });
                Ok((loaded, remaining))
            },
        )
        .map(|(loaded, _remaining)| loaded)
}

fn load_control(path: Option<&Path>) -> Result<Option<AcquiredControl>, ConfigError> {
    path.map(|path| {
        read_regular(path, REQUEST_STREAM_BYTES).map(|bytes| AcquiredControl {
            bytes,
            trust_source: RequestTrust::OrganizationPolicy,
        })
    })
    .transpose()
}
