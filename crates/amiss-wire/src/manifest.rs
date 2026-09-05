use serde::{Deserialize, Serialize};

use crate::controls::{ConstraintPlatform, GitMode, root, sorted_set, validate_repository};
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

pub const MANIFEST_DOMAIN: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_DOMAIN: &str = "amiss/scanner-dependency-lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseManifestSchema {
    #[serde(rename = "amiss/scanner-release-manifest")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyLockSchema {
    #[serde(rename = "amiss/scanner-dependency-lock-input")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeContract {
    #[serde(rename = "manifest-closed")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentContract {
    #[serde(rename = "scanner-process-env")]
    Current,
}

/// One runtime file of the reviewed action closure: a regular blob in the
/// pinned action tree with its exact mode and plain SHA-256.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFile {
    pub file_sha256: Digest,
    pub git_mode: GitMode,
    pub path: RepoPathText,
    pub role: RuntimeRole,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::EnumString,
    strum::IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeRole {
    Executable,
    DynamicLibrary,
    RuntimeData,
}

/// One published platform artifact and its complete runtime closure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub artifact_name: ArtifactId,
    pub binary_sha256: Digest,
    pub engine_digest: Digest,
    pub environment_contract: EnvironmentContract,
    pub platform: ConstraintPlatform,
    pub runtime_contract: RuntimeContract,
    pub runtime_files: Vec<RuntimeFile>,
    pub tree_path: RepoPathText,
}

/// The build namespace: the repository and exact commit the release was
/// built from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSource {
    pub commit_oid: Oid,
    pub object_format: ObjectFormat,
    pub repository: RepositoryIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyLockFile {
    pub path: RepoPathText,
    pub raw_digest: Digest,
}

/// Every build lockfile by canonical path and raw-evidence digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyLockInput {
    pub files: Vec<DependencyLockFile>,
    pub schema: DependencyLockSchema,
}

/// The strict release manifest: the reviewed release label, its build
/// namespace, the complete dependency-lock set, and one to six artifacts
/// sorted by platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub artifacts: Vec<ReleaseArtifact>,
    pub build_source: BuildSource,
    pub dependency_lock: DependencyLockInput,
    pub dependency_lock_digest: Digest,
    pub engine_version: String,
    pub schema: ReleaseManifestSchema,
}

/// Parses and validates one release manifest.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, invalid grammar
/// values, inconsistent digests or closure rows, and unsorted or duplicate
/// set members.
pub fn parse_release_manifest(bytes: &[u8]) -> Result<ReleaseManifest, Error> {
    root(bytes)?;
    let manifest = de::deserialize_json(bytes)?;
    validate_release_manifest(&manifest)?;
    Ok(manifest)
}

/// Produces one valid release manifest's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_release_manifest`] enforces,
/// or the typed value cannot be serialized.
pub fn canonical_release_manifest(manifest: &ReleaseManifest) -> Result<(Vec<u8>, Digest), Error> {
    validate_release_manifest(manifest)?;
    let bytes = serde_json_canonicalizer::to_vec(manifest)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(MANIFEST_DOMAIN, &bytes);
    Ok((bytes, digest))
}

/// Produces one valid dependency-lock input's canonical bytes and digest.
///
/// # Errors
///
/// The lock set is empty, oversized, unsorted, duplicated, or cannot be
/// serialized.
pub fn canonical_dependency_lock(
    dependency_lock: &DependencyLockInput,
) -> Result<(Vec<u8>, Digest), Error> {
    validate_dependency_lock("$", dependency_lock)?;
    let bytes = serde_json_canonicalizer::to_vec(dependency_lock)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(DEPENDENCY_LOCK_DOMAIN, &bytes);
    Ok((bytes, digest))
}

fn validate_release_manifest(manifest: &ReleaseManifest) -> Result<(), Error> {
    if !valid_version(&manifest.engine_version) {
        return fail("$.engine_version", ErrorKind::InvalidValue);
    }
    validate_repository(
        "$.build_source.repository",
        &manifest.build_source.repository,
    )?;
    if manifest.build_source.commit_oid.object_format() != manifest.build_source.object_format {
        return fail("$.build_source.commit_oid", ErrorKind::InvalidValue);
    }
    validate_dependency_lock("$.dependency_lock", &manifest.dependency_lock)?;
    let lock_bytes = serde_json_canonicalizer::to_vec(&manifest.dependency_lock)
        .map_err(|_defect| Error::new("$.dependency_lock", ErrorKind::InvalidValue))?;
    if hb(DEPENDENCY_LOCK_DOMAIN, &lock_bytes) != manifest.dependency_lock_digest {
        return fail("$.dependency_lock_digest", ErrorKind::DigestMismatch);
    }
    if manifest.artifacts.is_empty() || manifest.artifacts.len() > 6 {
        return fail("$.artifacts", ErrorKind::LimitExceeded);
    }
    for (index, artifact) in manifest.artifacts.iter().enumerate() {
        validate_release_artifact(&format!("$.artifacts[{index}]"), artifact)?;
    }
    sorted_set("$.artifacts", &manifest.artifacts, |left, right| {
        left.platform.as_ref().cmp(right.platform.as_ref())
    })
}

fn validate_dependency_lock(
    path: &str,
    dependency_lock: &DependencyLockInput,
) -> Result<(), Error> {
    let files_path = format!("{path}.files");
    if dependency_lock.files.is_empty() || dependency_lock.files.len() > 32 {
        return fail(&files_path, ErrorKind::LimitExceeded);
    }
    sorted_set(&files_path, &dependency_lock.files, |left, right| {
        left.path.as_str().cmp(right.path.as_str())
    })
}

fn validate_release_artifact(path: &str, artifact: &ReleaseArtifact) -> Result<(), Error> {
    let files_path = format!("{path}.runtime_files");
    if artifact.runtime_files.is_empty() || artifact.runtime_files.len() > 256 {
        return fail(&files_path, ErrorKind::LimitExceeded);
    }
    for (index, file) in artifact.runtime_files.iter().enumerate() {
        if !matches!(
            file.git_mode,
            GitMode::RegularFile | GitMode::ExecutableFile
        ) {
            return fail(
                &format!("{files_path}[{index}].git_mode"),
                ErrorKind::InvalidValue,
            );
        }
    }
    sorted_set(&files_path, &artifact.runtime_files, |left, right| {
        left.path.as_str().cmp(right.path.as_str())
    })?;
    let mut executable = artifact
        .runtime_files
        .iter()
        .filter(|file| file.role == RuntimeRole::Executable);
    let row = executable.next();
    if executable.next().is_some()
        || row.is_none_or(|file| {
            file.path != artifact.tree_path
                || file.git_mode != GitMode::ExecutableFile
                || file.file_sha256 != artifact.binary_sha256
        })
    {
        return fail(&files_path, ErrorKind::Inconsistent);
    }
    Ok(())
}

fn valid_version(raw: &str) -> bool {
    let (core, pre) = raw
        .split_once('-')
        .map_or((raw, None), |(core, pre)| (core, Some(pre)));
    let mut numeric = core.split('.');
    raw.len() <= 64
        && (0..3).all(|_| {
            numeric.next().is_some_and(|part| {
                !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        && numeric.next().is_none()
        && pre.is_none_or(|text| {
            !text.is_empty()
                && text.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'-'
                })
        })
}
