pub mod build;
pub mod result;
pub mod supervise;

use amiss_git::{GitResources, ObjectKind, Repository};
use amiss_wire::action::{executable_platform, host_platform};
use amiss_wire::controls::{ConstraintPlatform, ExecutionConstraintDescriptor, GitMode};
use amiss_wire::digest::{Digest, RAW_EVIDENCE_DOMAIN, hb, sha256};
use amiss_wire::manifest::{ReleaseArtifact, ReleaseManifest, RuntimeRole};
use amiss_wire::model::{Oid, RepoPathText};

/// The engine names itself with this domain over its own bytes, and the
/// bootstrap recomputes the same value over the binary it validated. One
/// definition, so the two can never drift apart and silently stop matching.
pub use amiss_wire::report::ENGINE_DOMAIN;

pub const BOOTSTRAP_DOMAIN: &str = "amiss/scanner-action-bootstrap";
pub const ACTION_METADATA_PATH: &str = "action.yml";

/// A validation refusal and the trust decision it requires. The diagnostic is
/// attached where the refusal originates, so adding a reason cannot leave a
/// downstream classifier incomplete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The trusted input was unavailable for this runner.
    Unavailable(&'static str),
    /// The trusted input contradicted its authenticated binding.
    Tampered(&'static str),
}

const fn tampered(reason: &'static str) -> Refusal {
    Refusal::Tampered(reason)
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
}

/// The trusted bootstrap's validation, in the contract's order: choose from
/// the closed platform table, resolve the reported commit to its reported
/// tree, resolve the metadata, manifest, artifact, every lockfile, and every
/// runtime path as regular non-symlink blobs in that tree, require the parsed
/// manifest to carry the constrained digest, require every recorded lockfile
/// to recompute from the tree's bytes, verify every mode and checksum, verify the
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
        return Err(tampered("bootstrap-digest-mismatch"));
    }
    let platform = host_platform().ok_or(Refusal::Unavailable("unsupported-platform"))?;
    if platform != constraint.selected_platform {
        return Err(tampered("platform-binding-mismatch"));
    }

    let tree = action_tree(action, resources, constraint)?;

    let (manifest_bytes, manifest_mode) =
        blob(action, resources, &tree, &constraint.manifest_path)?;
    if manifest_mode != GitMode::RegularFile {
        return Err(tampered("path-not-regular-blob"));
    }
    let manifest = ReleaseManifest::parse(&manifest_bytes)
        .map_err(|_defect| tampered("manifest-unreadable"))?;
    if manifest.digest != constraint.release_manifest_digest {
        return Err(tampered("manifest-digest-mismatch"));
    }

    // The manifest's lock set is parse-bound to its own set digest, so what is
    // left to prove is that the tree really carries those bytes: each recorded
    // lockfile re-resolved as a regular blob and re-hashed under the same
    // domain the release builder used. Without this, the one file that says
    // which dependencies built the engine is the one file nothing checks.
    for (lock_path, lock_digest) in &manifest.dependency_lock.files {
        let (lock_bytes, lock_mode) = blob(action, resources, &tree, lock_path)?;
        if lock_mode != GitMode::RegularFile {
            return Err(tampered("path-not-regular-blob"));
        }
        if hb(RAW_EVIDENCE_DOMAIN, &lock_bytes) != *lock_digest {
            return Err(tampered("dependency-lock-mismatch"));
        }
    }

    let artifact = manifest
        .artifacts
        .iter()
        .find(|candidate| candidate.platform == platform)
        .cloned()
        .ok_or(tampered("artifact-selection-failed"))?;

    let mut binary: Option<Vec<u8>> = None;
    for file in &artifact.runtime_files {
        let (bytes, mode) = blob(action, resources, &tree, &file.path)?;
        if mode != file.git_mode {
            return Err(tampered("runtime-closure-mismatch"));
        }
        if sha256(&bytes) != file.file_sha256 {
            return Err(tampered("runtime-closure-mismatch"));
        }
        if file.role == RuntimeRole::Executable {
            binary = Some(bytes);
        }
    }
    let binary = binary.ok_or(tampered("runtime-closure-mismatch"))?;

    if sha256(&binary) != artifact.binary_sha256 {
        return Err(tampered("engine-digest-mismatch"));
    }
    let engine_digest = hb(ENGINE_DOMAIN, &binary);
    if engine_digest != artifact.engine_digest {
        return Err(tampered("engine-digest-mismatch"));
    }
    if executable_platform(&binary) != Some(platform) {
        return Err(tampered("platform-binding-mismatch"));
    }
    let _launcher = artifact
        .launcher()
        .ok_or(tampered("runtime-closure-mismatch"))?;
    // the one runnable file at the tree root must be a pinned closure row,
    // or the action a workflow executes is the one file nothing checks
    let action_pinned = artifact.runtime_files.iter().any(|file| {
        file.role == RuntimeRole::RuntimeData && file.path.as_str() == ACTION_METADATA_PATH
    });
    if !action_pinned {
        return Err(tampered("action-metadata-invalid"));
    }

    Ok(Validated {
        manifest,
        artifact,
        platform,
        engine_digest,
        binary,
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
        return Err(tampered("action-tree-mismatch"));
    }
    let object = action
        .read_expected(resources, &constraint.action_commit_oid, ObjectKind::Commit)
        .map_err(|_defect| tampered("action-tree-mismatch"))?;
    let commit = amiss_git::parse_commit(action.object_format(), &object.body)
        .map_err(|_defect| tampered("action-tree-mismatch"))?;
    if commit.tree != constraint.action_tree_oid {
        return Err(tampered("action-tree-mismatch"));
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
    path: &RepoPathText,
) -> Result<(Vec<u8>, GitMode), Refusal> {
    let mut current = tree.clone();
    let mut segments = path.as_str().split('/').peekable();
    while let Some(segment) = segments.next() {
        let object = action
            .read_expected(resources, &current, ObjectKind::Tree)
            .map_err(|_defect| tampered("path-not-regular-blob"))?;
        let entries = amiss_git::parse_tree(action.object_format(), &object.body)
            .map_err(|_defect| tampered("path-not-regular-blob"))?;
        let entry = entries
            .iter()
            .find(|entry| entry.name == segment.as_bytes())
            .ok_or(tampered("path-not-regular-blob"))?;
        if segments.peek().is_some() {
            if entry.mode != GitMode::Tree {
                return Err(tampered("path-not-regular-blob"));
            }
            current = entry.oid.clone();
            continue;
        }
        let mode = entry.mode;
        if mode != GitMode::RegularFile && mode != GitMode::ExecutableFile {
            return Err(tampered("path-not-regular-blob"));
        }
        let object = action
            .read_expected(resources, &entry.oid, ObjectKind::Blob)
            .map_err(|_defect| tampered("path-not-regular-blob"))?;
        return Ok((object.body, mode));
    }
    Err(tampered("path-not-regular-blob"))
}
