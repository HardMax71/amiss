mod tests;

use std::fs::FileType;
use std::path::{Path, PathBuf};

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{BOOTSTRAP_EXECUTABLE_BYTES, CheckPlan};
use amiss_wire::action::host_platform;
use amiss_wire::digest::hb;
use serde::Deserialize;

use super::{ConfigError, read_regular};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServicePaths {
    bootstrap: PathBuf,
    scratch: PathBuf,
    inbox: PathBuf,
    ledger: PathBuf,
    artifacts: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPaths {
    bootstrap: PathBuf,
    scratch: PathBuf,
    ledger: PathBuf,
    artifacts: PathBuf,
}

pub struct LoadedPaths {
    pub bootstrap: PathBuf,
    pub scratch: PathBuf,
    pub inbox: PathBuf,
    pub ledger: PathBuf,
    pub artifacts: PathBuf,
}

pub struct LoadedExecutionPaths {
    pub bootstrap: PathBuf,
    pub scratch: PathBuf,
    pub ledger: PathBuf,
    pub artifacts: PathBuf,
}

/// Resolves separate private state roots and binds the bootstrap executable.
///
/// # Errors
///
/// A path is relative, symlinked, inaccessible, of the wrong kind, overlaps a
/// state root, or names bootstrap bytes outside the plan.
pub fn load_paths(paths: &ServicePaths, plan: &CheckPlan) -> Result<LoadedPaths, ConfigError> {
    let overlap_context = "scratch, inbox, ledger, and artifact roots must be separate";
    let execution = load_execution_roots(
        &paths.bootstrap,
        &paths.scratch,
        &paths.ledger,
        &paths.artifacts,
        plan,
        overlap_context,
    )?;
    let directory = PathRequirements {
        accepts: FileType::is_dir,
        invalid: "state and scratch paths must be real absolute directories",
        unresolved: "a state root cannot be resolved",
    };
    let inbox = canonical_path(&paths.inbox, directory)?;
    separate_roots(
        [
            &execution.scratch,
            &inbox,
            &execution.ledger,
            &execution.artifacts,
        ],
        overlap_context,
    )?;
    Ok(LoadedPaths {
        bootstrap: execution.bootstrap,
        scratch: execution.scratch,
        inbox,
        ledger: execution.ledger,
        artifacts: execution.artifacts,
    })
}

/// Resolves separate execution roots and binds the bootstrap executable.
///
/// # Errors
///
/// A path is relative, symlinked, inaccessible, of the wrong kind, overlaps
/// the other state root, or names bootstrap bytes outside the plan.
pub fn load_execution_paths(
    paths: &ExecutionPaths,
    plan: &CheckPlan,
) -> Result<LoadedExecutionPaths, ConfigError> {
    load_execution_roots(
        &paths.bootstrap,
        &paths.scratch,
        &paths.ledger,
        &paths.artifacts,
        plan,
        "scratch, ledger, and artifact roots must be separate",
    )
}

fn load_execution_roots(
    bootstrap: &Path,
    scratch: &Path,
    ledger: &Path,
    artifacts: &Path,
    plan: &CheckPlan,
    overlap_context: &'static str,
) -> Result<LoadedExecutionPaths, ConfigError> {
    let execution = resolve_execution_paths(bootstrap, scratch, ledger, artifacts, plan)?;
    separate_roots(
        [&execution.scratch, &execution.ledger, &execution.artifacts],
        overlap_context,
    )?;
    Ok(execution)
}

fn resolve_execution_paths(
    bootstrap: &Path,
    scratch: &Path,
    ledger: &Path,
    artifacts: &Path,
    plan: &CheckPlan,
) -> Result<LoadedExecutionPaths, ConfigError> {
    let directory = PathRequirements {
        accepts: FileType::is_dir,
        invalid: "state and scratch paths must be real absolute directories",
        unresolved: "a state root cannot be resolved",
    };
    let scratch = canonical_path(scratch, directory)?;
    let ledger = canonical_path(ledger, directory)?;
    let artifacts = canonical_path(artifacts, directory)?;
    let bootstrap = canonical_path(
        bootstrap,
        PathRequirements {
            accepts: FileType::is_file,
            invalid: "bootstrap must be one real absolute file",
            unresolved: "bootstrap cannot be resolved",
        },
    )?;
    let bootstrap_bytes = read_regular(&bootstrap, BOOTSTRAP_EXECUTABLE_BYTES)?;
    (hb(BOOTSTRAP_DOMAIN, &bootstrap_bytes) == plan.execution.bootstrap_digest)
        .then_some(())
        .ok_or(ConfigError::invalid(
            "bootstrap does not match the execution constraint",
        ))?;
    (host_platform() == Some(plan.execution.selected_platform))
        .then_some(LoadedExecutionPaths {
            bootstrap,
            scratch,
            ledger,
            artifacts,
        })
        .ok_or(ConfigError::invalid(
            "execution constraint does not target this host",
        ))
}

#[derive(Clone, Copy)]
struct PathRequirements {
    accepts: fn(&FileType) -> bool,
    invalid: &'static str,
    unresolved: &'static str,
}

fn canonical_path(path: &Path, requirements: PathRequirements) -> Result<PathBuf, ConfigError> {
    let valid = path.is_absolute()
        && std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| (requirements.accepts)(&metadata.file_type()));
    if !valid {
        return Err(ConfigError::invalid(requirements.invalid));
    }
    std::fs::canonicalize(path)
        .map_err(|defect| ConfigError::caused_by(requirements.unresolved, defect))
}

fn separate_roots<const N: usize>(
    roots: [&Path; N],
    overlap_context: &'static str,
) -> Result<(), ConfigError> {
    let overlap = roots.iter().enumerate().any(|(position, root)| {
        roots
            .iter()
            .skip(position.saturating_add(1))
            .any(|other| root.starts_with(other) || other.starts_with(root))
    });
    (!overlap)
        .then_some(())
        .ok_or(ConfigError::invalid(overlap_context))
}
