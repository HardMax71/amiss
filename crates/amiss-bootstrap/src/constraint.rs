use amiss_git::{GitResources, Repository};
use amiss_wire::action::executable_platform;
use amiss_wire::controls::{
    ActionBootstrapContract, ExecutionConstraintDescriptor, ExecutionConstraintSchema, GitMode,
    canonical_execution_constraint,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{Oid, RepoPathText, RepositoryIdentity};

use crate::build::{RELEASE_MANIFEST_DIGEST_PATH, RELEASE_MANIFEST_PATH};
use crate::{
    BOOTSTRAP_DOMAIN, Refusal, blob, load_release_manifest, resolve_action_tree, validate_release,
};

/// A provisioning failure with no runtime trust classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct ConstraintError {
    /// Stable diagnostic for the failed derivation step.
    pub reason: &'static str,
}

/// Derives the existing execution-constraint contract from one exact action
/// commit and one bootstrap executable, then validates every dependency lock
/// and the selected platform's runtime closure before returning it.
///
/// # Errors
///
/// The action commit, manifest marker, dependency locks, selected runtime,
/// bootstrap platform, or supplied constraint fields are unavailable or
/// inconsistent.
pub fn derive_execution_constraint(
    action: &Repository,
    resources: &mut GitResources,
    action_repository: &RepositoryIdentity,
    action_commit_oid: &Oid,
    required_status_name: &str,
    bootstrap_bytes: &[u8],
) -> Result<ExecutionConstraintDescriptor, ConstraintError> {
    let platform = executable_platform(bootstrap_bytes).ok_or(ConstraintError {
        reason: "bootstrap-platform-mismatch",
    })?;
    let tree =
        resolve_action_tree(action, resources, action_commit_oid).map_err(constraint_error)?;
    let manifest_path =
        RepoPathText::new(RELEASE_MANIFEST_PATH.to_owned()).ok_or(ConstraintError {
            reason: "execution-constraint-invalid",
        })?;
    let manifest = load_release_manifest(action, resources, &tree, &manifest_path)
        .map_err(constraint_error)?;
    let marker_path =
        RepoPathText::new(RELEASE_MANIFEST_DIGEST_PATH.to_owned()).ok_or(ConstraintError {
            reason: "execution-constraint-invalid",
        })?;
    let (marker, mode) = blob(action, resources, &tree, &marker_path).map_err(constraint_error)?;
    if mode != GitMode::RegularFile {
        return Err(ConstraintError {
            reason: "manifest-digest-mismatch",
        });
    }
    let marked_digest = parse_marker(&marker).ok_or(ConstraintError {
        reason: "manifest-digest-mismatch",
    })?;
    if marked_digest != manifest.digest {
        return Err(ConstraintError {
            reason: "manifest-digest-mismatch",
        });
    }
    let descriptor = ExecutionConstraintDescriptor {
        schema: ExecutionConstraintSchema::Current,
        action_repository: action_repository.clone(),
        action_object_format: action.object_format(),
        action_commit_oid: action_commit_oid.clone(),
        action_tree_oid: tree.clone(),
        manifest_path,
        release_manifest_digest: manifest.digest,
        selected_platform: platform,
        required_status_name: required_status_name.to_owned(),
        bootstrap_contract: ActionBootstrapContract::Current,
        bootstrap_digest: hb(BOOTSTRAP_DOMAIN, bootstrap_bytes),
    };
    canonical_execution_constraint(&descriptor).map_err(|_defect| ConstraintError {
        reason: "execution-constraint-invalid",
    })?;
    validate_release(action, resources, &tree, manifest, platform).map_err(constraint_error)?;
    Ok(descriptor)
}

const fn constraint_error(refusal: Refusal) -> ConstraintError {
    match refusal {
        Refusal::Unavailable(reason) | Refusal::Tampered(reason) => ConstraintError { reason },
    }
}

fn parse_marker(bytes: &[u8]) -> Option<Digest> {
    bytes
        .strip_suffix(b"\n")
        .and_then(|text| std::str::from_utf8(text).ok())
        .and_then(Digest::from_wire)
}
