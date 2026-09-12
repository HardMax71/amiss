use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::codec;
use crate::controls::{ConstraintPlatform, GitMode};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

pub const MANIFEST_SCHEMA: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_SCHEMA: &str = "amiss/scanner-dependency-lock-input";
pub const MANIFEST_DOMAIN: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_DOMAIN: &str = "amiss/scanner-dependency-lock";
pub const RUNTIME_CONTRACT: &str = "manifest-closed";
pub const ENVIRONMENT_CONTRACT: &str = "scanner-process-env";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFile {
    pub path: RepoPathText,
    pub role: RuntimeRole,
    pub git_mode: GitMode,
    pub file_sha256: Digest,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseArtifact {
    pub platform: ConstraintPlatform,
    pub artifact_name: ArtifactId,
    pub tree_path: RepoPathText,
    pub binary_sha256: Digest,
    pub engine_digest: Digest,
    pub runtime_files: Vec<RuntimeFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSource {
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    pub commit_oid: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyLockInput {
    pub files: Vec<(RepoPathText, Digest)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub digest: Digest,
    pub engine_version: String,
    pub build_source: BuildSource,
    pub dependency_lock: DependencyLockInput,
    pub dependency_lock_digest: Digest,
    pub artifacts: Vec<ReleaseArtifact>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDocument {
    schema: String,
    engine_version: String,
    build_source: BuildSource,
    dependency_lock: LockDocument,
    dependency_lock_digest: Digest,
    artifacts: Vec<ArtifactDocument>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockDocument {
    schema: String,
    files: Vec<LockFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockFile {
    path: RepoPathText,
    raw_digest: Digest,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDocument {
    platform: ConstraintPlatform,
    artifact_name: ArtifactId,
    tree_path: RepoPathText,
    binary_sha256: Digest,
    engine_digest: Digest,
    runtime_contract: String,
    environment_contract: String,
    runtime_files: Vec<RuntimeFile>,
}

impl ReleaseManifest {
    /// Parses the closed manifest and checks ordering, runtime closure, and lock-set digest.
    ///
    /// # Errors
    ///
    /// The document has a syntax, shape, ordering, identity, or digest defect.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::decode(&codec::decode(bytes)?)
    }

    /// Decodes an embedded manifest without serializing and reparsing it.
    ///
    /// # Errors
    ///
    /// As [`Self::parse`].
    pub fn decode(value: &Value) -> Result<Self, Error> {
        let digest = codec::digest(MANIFEST_DOMAIN, value)?;
        let document: ManifestDocument = codec::from_value("$", value)?;
        if document.schema != MANIFEST_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        validate_version(&document.engine_version)?;
        if document.build_source.commit_oid.object_format() != document.build_source.object_format {
            return fail("$.build_source.commit_oid", ErrorKind::InvalidValue);
        }
        if document.dependency_lock.schema != DEPENDENCY_LOCK_SCHEMA {
            return fail("$.dependency_lock.schema", ErrorKind::InvalidValue);
        }
        let files = &document.dependency_lock.files;
        bounded_set("$.dependency_lock.files", files, 32, |a, b| {
            a.path.cmp(&b.path)
        })?;
        if codec::digest(DEPENDENCY_LOCK_DOMAIN, &document.dependency_lock)?
            != document.dependency_lock_digest
        {
            return fail("$.dependency_lock_digest", ErrorKind::InvalidValue);
        }
        bounded_set("$.artifacts", &document.artifacts, 6, |a, b| {
            a.platform.as_ref().cmp(b.platform.as_ref())
        })?;
        let artifacts = document
            .artifacts
            .into_iter()
            .enumerate()
            .map(|(index, artifact)| artifact.checked(&format!("$.artifacts[{index}]")))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            digest,
            engine_version: document.engine_version,
            build_source: document.build_source,
            dependency_lock: DependencyLockInput {
                files: document
                    .dependency_lock
                    .files
                    .into_iter()
                    .map(|file| (file.path, file.raw_digest))
                    .collect(),
            },
            dependency_lock_digest: document.dependency_lock_digest,
            artifacts,
        })
    }

    #[must_use]
    pub fn select(
        &self,
        platform: ConstraintPlatform,
        name: &ArtifactId,
    ) -> Option<&ReleaseArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.platform == platform && &artifact.artifact_name == name)
    }
}

impl ArtifactDocument {
    fn checked(self, path: &str) -> Result<ReleaseArtifact, Error> {
        if self.runtime_contract != RUNTIME_CONTRACT {
            return fail(&format!("{path}.runtime_contract"), ErrorKind::InvalidValue);
        }
        if self.environment_contract != ENVIRONMENT_CONTRACT {
            return fail(
                &format!("{path}.environment_contract"),
                ErrorKind::InvalidValue,
            );
        }
        let files_path = format!("{path}.runtime_files");
        bounded_set(&files_path, &self.runtime_files, 256, |a, b| {
            a.path.cmp(&b.path)
        })?;
        for (index, file) in self.runtime_files.iter().enumerate() {
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
        let artifact = ReleaseArtifact {
            platform: self.platform,
            artifact_name: self.artifact_name,
            tree_path: self.tree_path,
            binary_sha256: self.binary_sha256,
            engine_digest: self.engine_digest,
            runtime_files: self.runtime_files,
        };
        if artifact.executable().is_none() {
            return fail(&files_path, ErrorKind::Inconsistent);
        }
        Ok(artifact)
    }
}

impl ReleaseArtifact {
    #[must_use]
    pub fn executable(&self) -> Option<&RuntimeFile> {
        let mut rows = self
            .runtime_files
            .iter()
            .filter(|file| file.role == RuntimeRole::Executable);
        let row = rows.next()?;
        if rows.next().is_some()
            || row.path != self.tree_path
            || row.git_mode != GitMode::ExecutableFile
            || row.file_sha256 != self.binary_sha256
        {
            return None;
        }
        Some(row)
    }
}

fn validate_version(raw: &str) -> Result<(), Error> {
    let (core, pre) = raw
        .split_once('-')
        .map_or((raw, None), |(core, pre)| (core, Some(pre)));
    let numeric: Vec<&str> = core.split('.').collect();
    if raw.len() <= 64
        && numeric.len() == 3
        && numeric
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && pre.is_none_or(|text| {
            !text.is_empty()
                && text.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'-'
                })
        })
    {
        Ok(())
    } else {
        fail("$.engine_version", ErrorKind::InvalidValue)
    }
}

fn bounded_set<T>(
    path: &str,
    items: &[T],
    maximum: usize,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), Error> {
    if items.is_empty() || items.len() > maximum {
        return fail(path, ErrorKind::LimitExceeded);
    }
    if items
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if compare(left, right) != Ordering::Less))
    {
        return fail(path, ErrorKind::UnsortedSet);
    }
    Ok(())
}
