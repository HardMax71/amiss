pub mod build;
pub mod supervise;

use amiss_git::{GitResources, ObjectKind, Repository};
use amiss_wire::action::{ActionMetadata, executable_platform, host_platform};
use amiss_wire::controls::{ConstraintPlatform, ExecutionConstraintDescriptor, GitMode};
use amiss_wire::digest::{Digest, hb, sha256};
use amiss_wire::manifest::{ReleaseArtifact, ReleaseManifest, RuntimeRole};
use amiss_wire::model::{Oid, RepoPath};

/// The engine names itself with this domain over its own bytes, and the
/// bootstrap recomputes the same value over the binary it validated. One
/// definition, so the two can never drift apart and silently stop matching.
pub use amiss_wire::report::ENGINE_DOMAIN;

pub const BOOTSTRAP_DOMAIN: &str = "amiss/scanner-action-bootstrap/v1";
pub const ACTION_METADATA_PATH: &str = "action.yml";

/// The exact reason a validation refused, in the order the contract checks
/// them. Every one fails before scanning; none of them is recoverable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The runner's platform is outside the closed table.
    UnsupportedPlatform,
    /// The action repository, commit, or tree does not resolve as reported.
    ActionTree,
    /// The root `action.yml` is not the restricted metadata shape.
    ActionMetadata,
    /// A required path is absent, a symlink, a gitlink, or the wrong mode.
    PathNotRegularBlob,
    /// The manifest blob does not parse under the strict rules.
    ManifestUnreadable,
    /// The manifest's semantic digest differs from the constraint.
    ManifestDigest,
    /// No artifact matches the selected platform and name.
    ArtifactSelection,
    /// A runtime file's checksum or mode differs from its manifest row.
    RuntimeClosure,
    /// The selected binary's SHA-256 or engine digest differs.
    EngineDigest,
    /// The executable's own header names a different platform.
    PlatformBinding,
    /// The bootstrap's own bytes do not recompute to the pinned digest.
    BootstrapDigest,
}

/// The validated release: everything the bootstrap proved before it is
/// allowed to exec anything.
#[derive(Clone, Debug)]
pub struct Validated {
    pub manifest: ReleaseManifest,
    pub artifact: ReleaseArtifact,
    pub platform: ConstraintPlatform,
    pub engine_digest: Digest,
    pub binary: Vec<u8>,
    pub metadata: ActionMetadata,
}

/// The trusted bootstrap's validation, in the contract's order: choose from
/// the closed platform table, resolve the reported commit to its reported
/// tree, resolve the metadata, manifest, artifact, and every runtime path as
/// regular non-symlink blobs in that tree, require the parsed manifest to
/// carry the constrained digest, verify every mode and checksum, verify the
/// selected binary's plain SHA-256 and domain-separated engine digest, and
/// require the executable's own header to name the same platform. Nothing is
/// downloaded, installed, discovered, or resolved through `PATH`.
///
/// # Errors
///
/// The first refusal above. A refusal always precedes execution.
pub fn validate(
    action: &Repository,
    resources: &mut GitResources,
    constraint: &ExecutionConstraintDescriptor,
    bootstrap_bytes: &[u8],
) -> Result<Validated, Refusal> {
    if hb(BOOTSTRAP_DOMAIN, bootstrap_bytes) != constraint.bootstrap_digest {
        return Err(Refusal::BootstrapDigest);
    }
    let platform = host_platform().ok_or(Refusal::UnsupportedPlatform)?;
    if platform != constraint.selected_platform {
        return Err(Refusal::PlatformBinding);
    }

    let tree = action_tree(action, resources, constraint)?;

    let metadata_path =
        RepoPath::new(ACTION_METADATA_PATH.to_owned()).ok_or(Refusal::ActionMetadata)?;
    let (metadata_bytes, metadata_mode) = blob(action, resources, &tree, &metadata_path)?;
    if metadata_mode != GitMode::RegularFile {
        return Err(Refusal::PathNotRegularBlob);
    }
    let metadata =
        ActionMetadata::parse(&metadata_bytes).map_err(|_defect| Refusal::ActionMetadata)?;

    let (manifest_bytes, manifest_mode) =
        blob(action, resources, &tree, &constraint.manifest_path)?;
    if manifest_mode != GitMode::RegularFile {
        return Err(Refusal::PathNotRegularBlob);
    }
    let manifest =
        ReleaseManifest::parse(&manifest_bytes).map_err(|_defect| Refusal::ManifestUnreadable)?;
    if manifest.digest != constraint.release_manifest_digest {
        return Err(Refusal::ManifestDigest);
    }

    let artifact = manifest
        .artifacts
        .iter()
        .find(|candidate| candidate.platform == platform)
        .cloned()
        .ok_or(Refusal::ArtifactSelection)?;

    let mut binary: Option<Vec<u8>> = None;
    for file in &artifact.runtime_files {
        let (bytes, mode) = blob(action, resources, &tree, &file.path)?;
        if mode != file.git_mode {
            return Err(Refusal::RuntimeClosure);
        }
        if sha256(&bytes) != file.file_sha256 {
            return Err(Refusal::RuntimeClosure);
        }
        if file.role == RuntimeRole::Executable {
            binary = Some(bytes);
        }
    }
    let binary = binary.ok_or(Refusal::RuntimeClosure)?;

    if sha256(&binary) != artifact.binary_sha256 {
        return Err(Refusal::EngineDigest);
    }
    let engine_digest = hb(ENGINE_DOMAIN, &binary);
    if engine_digest != artifact.engine_digest {
        return Err(Refusal::EngineDigest);
    }
    if executable_platform(&binary) != Some(platform) {
        return Err(Refusal::PlatformBinding);
    }
    if artifact.launcher().ok_or(Refusal::RuntimeClosure)?.path != metadata.main {
        return Err(Refusal::ActionMetadata);
    }

    Ok(Validated {
        manifest,
        artifact,
        platform,
        engine_digest,
        binary,
        metadata,
    })
}

/// Resolves the reported action commit to its reported tree, requiring the
/// commit to exist in the pinned object format and its tree OID to equal the
/// reported one.
fn action_tree(
    action: &Repository,
    resources: &mut GitResources,
    constraint: &ExecutionConstraintDescriptor,
) -> Result<Oid, Refusal> {
    if action.object_format() != constraint.action_object_format {
        return Err(Refusal::ActionTree);
    }
    let object = action
        .read_expected(resources, &constraint.action_commit_oid, ObjectKind::Commit)
        .map_err(|_defect| Refusal::ActionTree)?;
    let commit = amiss_git::parse_commit(action.object_format(), &object.body)
        .map_err(|_defect| Refusal::ActionTree)?;
    if commit.tree != constraint.action_tree_oid {
        return Err(Refusal::ActionTree);
    }
    Ok(commit.tree)
}

/// Resolves one path in the pinned tree as a regular non-symlink blob,
/// returning its bytes and its exact Git mode. Symlinks, gitlinks,
/// directories, and absent entries all refuse.
fn blob(
    action: &Repository,
    resources: &mut GitResources,
    tree: &Oid,
    path: &RepoPath,
) -> Result<(Vec<u8>, GitMode), Refusal> {
    let mut current = tree.clone();
    let mut segments = path.as_str().split('/').peekable();
    while let Some(segment) = segments.next() {
        let object = action
            .read_expected(resources, &current, ObjectKind::Tree)
            .map_err(|_defect| Refusal::PathNotRegularBlob)?;
        let entries = amiss_git::parse_tree(action.object_format(), &object.body)
            .map_err(|_defect| Refusal::PathNotRegularBlob)?;
        let entry = entries
            .iter()
            .find(|entry| entry.name == segment.as_bytes())
            .ok_or(Refusal::PathNotRegularBlob)?;
        if segments.peek().is_some() {
            if entry.mode != GitMode::Tree {
                return Err(Refusal::PathNotRegularBlob);
            }
            current = entry.oid.clone();
            continue;
        }
        let mode = entry.mode;
        if mode != GitMode::RegularFile && mode != GitMode::ExecutableFile {
            return Err(Refusal::PathNotRegularBlob);
        }
        let object = action
            .read_expected(resources, &entry.oid, ObjectKind::Blob)
            .map_err(|_defect| Refusal::PathNotRegularBlob)?;
        return Ok((object.body, mode));
    }
    Err(Refusal::PathNotRegularBlob)
}
