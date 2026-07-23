use std::path::{Path, PathBuf};

use amiss_controller::{AcquiredControl, CheckPlan, PolicyControls, check_plan};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::requests::{REQUEST_STREAM_BYTES, RequestTrust};
use serde::Deserialize;

use super::{ConfigError, read_regular};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPlanFiles {
    profile: String,
    execution_constraint_file: PathBuf,
    organization_floor_file: Option<PathBuf>,
    debt_snapshot_file: Option<PathBuf>,
    waiver_bundle_file: Option<PathBuf>,
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
        _ => return Err(ConfigError("profile must be observe or enforce")),
    };
    let execution_bytes = read_regular(&raw.execution_constraint_file, REQUEST_STREAM_BYTES)?;
    let execution = ExecutionConstraintDescriptor::parse(&execution_bytes)
        .map_err(|_defect| ConfigError("execution constraint is invalid"))?;
    let policy = PolicyControls {
        organization_floor: load_control(raw.organization_floor_file.as_deref())?,
        debt_snapshot: load_control(raw.debt_snapshot_file.as_deref())?,
        waiver_bundle: load_control(raw.waiver_bundle_file.as_deref())?,
    };
    check_plan(profile, policy, execution).map_err(|_defect| ConfigError("check plan is invalid"))
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
