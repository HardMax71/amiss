use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::controls::{ConstraintPlatform, GitMode, sorted_set, validate_repository};
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;
use crate::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

pub const MANIFEST_DOMAIN: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_DOMAIN: &str = "amiss/scanner-dependency-lock";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ReleaseManifestSchema {
    #[strum(serialize = "amiss/scanner-release-manifest")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum DependencyLockSchema {
    #[strum(serialize = "amiss/scanner-dependency-lock-input")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum RuntimeContract {
    #[strum(serialize = "manifest-closed")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EnvironmentContract {
    #[strum(serialize = "scanner-process-env")]
    Current,
}

/// One runtime file of the reviewed action closure: a regular blob in the
/// pinned action tree with its exact mode and plain SHA-256.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct RuntimeFile {
    pub file_sha256: Digest,
    pub git_mode: GitMode,
    pub path: RepoPathText,
    pub role: RuntimeRole,
}

impl Serialize for RuntimeFile {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RuntimeFile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    strum::AsRefStr,
    EnumString,
    strum::IntoStaticStr,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RuntimeRole {
    Executable,
    DynamicLibrary,
    RuntimeData,
}

/// One published platform artifact and its complete runtime closure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
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

impl Serialize for ReleaseArtifact {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ReleaseArtifact {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// The build namespace: the repository and exact commit the release was
/// built from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct BuildSource {
    pub commit_oid: Oid,
    pub object_format: ObjectFormat,
    pub repository: RepositoryIdentity,
}

impl Serialize for BuildSource {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for BuildSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct DependencyLockFile {
    pub path: RepoPathText,
    pub raw_digest: Digest,
}

impl Serialize for DependencyLockFile {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for DependencyLockFile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Every build lockfile by canonical path and raw-evidence digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct DependencyLockInput {
    pub files: Vec<DependencyLockFile>,
    pub schema: DependencyLockSchema,
}

impl Serialize for DependencyLockInput {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for DependencyLockInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// The strict release manifest: the reviewed release label, its build
/// namespace, the complete dependency-lock set, and one to six artifacts
/// sorted by platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct ReleaseManifest {
    pub artifacts: Vec<ReleaseArtifact>,
    pub build_source: BuildSource,
    pub dependency_lock: DependencyLockInput,
    pub dependency_lock_digest: Digest,
    pub engine_version: String,
    pub schema: ReleaseManifestSchema,
}

impl Serialize for ReleaseManifest {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ReleaseManifest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses and validates one release manifest.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, invalid grammar
/// values, inconsistent digests or closure rows, and unsorted or duplicate
/// set members.
pub fn parse_release_manifest(bytes: &[u8]) -> Result<ReleaseManifest, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let manifest: ReleaseManifest = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    manifest.validate()?;
    Ok(manifest)
}

impl ReleaseManifest {
    /// Checks the build identity, dependency lock binding and runtime closure.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_release_manifest`].
    pub fn validate(&self) -> Result<(), Error> {
        if !valid_version(&self.engine_version) {
            return fail("$.engine_version", ErrorKind::InvalidValue);
        }
        validate_repository("$.build_source.repository", &self.build_source.repository)?;
        if self.build_source.commit_oid.object_format() != self.build_source.object_format {
            return fail("$.build_source.commit_oid", ErrorKind::InvalidValue);
        }
        validate_dependency_lock("$.dependency_lock", &self.dependency_lock)?;
        let mut writer = digest_io::IoWrapper(
            sha2::Sha256::new_with_prefix(DEPENDENCY_LOCK_DOMAIN).chain_update([0_u8]),
        );
        serde_json_canonicalizer::to_writer(&self.dependency_lock, &mut writer)
            .map_err(|_defect| Error::new("$.dependency_lock", ErrorKind::InvalidValue))?;
        if Digest::from(writer.0.finalize().0) != self.dependency_lock_digest {
            return fail("$.dependency_lock_digest", ErrorKind::DigestMismatch);
        }
        if self.artifacts.is_empty() || self.artifacts.len() > 6 {
            return fail("$.artifacts", ErrorKind::LimitExceeded);
        }
        for (index, artifact) in self.artifacts.iter().enumerate() {
            validate_release_artifact(&format!("$.artifacts[{index}]"), artifact)?;
        }
        sorted_set("$.artifacts", &self.artifacts, |left, right| {
            left.platform.as_ref().cmp(right.platform.as_ref())
        })
    }
}

impl DependencyLockInput {
    /// Checks that the lock set is nonempty, bounded, sorted and unique.
    ///
    /// # Errors
    ///
    /// The lockfiles violate a set or resource limit.
    pub fn validate(&self) -> Result<(), Error> {
        validate_dependency_lock("$", self)
    }
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
