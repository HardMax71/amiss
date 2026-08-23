use std::cmp::Ordering;

use crate::controls::{ConstraintPlatform, GitMode, decode_enum, root};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

pub const MANIFEST_SCHEMA: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_SCHEMA: &str = "amiss/scanner-dependency-lock-input";
pub const MANIFEST_DOMAIN: &str = "amiss/scanner-release-manifest";
pub const DEPENDENCY_LOCK_DOMAIN: &str = "amiss/scanner-dependency-lock";
pub const RUNTIME_CONTRACT: &str = "manifest-closed";
pub const ENVIRONMENT_CONTRACT: &str = "scanner-process-env";

/// One runtime file of the reviewed action closure: a regular blob in the
/// pinned action tree with its exact mode and plain SHA-256.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeFile {
    pub path: RepoPathText,
    pub role: RuntimeRole,
    pub git_mode: GitMode,
    pub file_sha256: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RuntimeRole {
    Executable,
    DynamicLibrary,
    RuntimeData,
}

/// One published platform artifact and its complete runtime closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseArtifact {
    pub platform: ConstraintPlatform,
    pub artifact_name: ArtifactId,
    pub tree_path: RepoPathText,
    pub binary_sha256: Digest,
    pub engine_digest: Digest,
    pub runtime_files: Vec<RuntimeFile>,
}

/// The build namespace: the repository and exact commit the release was
/// built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSource {
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    pub commit_oid: Oid,
}

/// Every build lockfile by canonical path and raw-evidence digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyLockInput {
    pub files: Vec<(RepoPathText, Digest)>,
}

/// The strict release manifest: the reviewed release label, its build
/// namespace, the complete dependency-lock set, and one to six artifacts
/// sorted by platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub digest: Digest,
    pub engine_version: String,
    pub build_source: BuildSource,
    pub dependency_lock: DependencyLockInput,
    pub dependency_lock_digest: Digest,
    pub artifacts: Vec<ReleaseArtifact>,
}

impl ReleaseManifest {
    /// Parses the manifest blob under the strict JSON rules, verifying the
    /// closed shape, the sorted-unique orders, and the lock-set digest.
    ///
    /// # Errors
    ///
    /// The first typed defect: shape, unknown field, invalid value, a
    /// noncanonical array order, a limit crossing, or a digest mismatch.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::decode(root(bytes)?)
    }

    /// Decodes an already-parsed value, which is how the wrapper checks the
    /// embedded manifest against the blob it parsed.
    ///
    /// # Errors
    ///
    /// As [`ReleaseManifest::parse`].
    pub fn decode(value: Value) -> Result<Self, Error> {
        let digest = hj(MANIFEST_DOMAIN, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, MANIFEST_SCHEMA)
        })?;
        let engine_version = obj.required("engine_version", decode_version)?;
        let build_source = obj.required("build_source", decode_build_source)?;
        let lock_path = obj.field("dependency_lock");
        let lock_value = obj.take("dependency_lock")?;
        let computed_lock = hj(DEPENDENCY_LOCK_DOMAIN, &lock_value);
        let dependency_lock = decode_lock(&lock_path, lock_value)?;
        let dependency_lock_digest = de::digest(
            &obj.field("dependency_lock_digest"),
            obj.take("dependency_lock_digest")?,
        )?;
        if dependency_lock_digest != computed_lock {
            return fail(
                &obj.field("dependency_lock_digest"),
                ErrorKind::InvalidValue,
            );
        }
        let artifacts_path = obj.field("artifacts");
        let artifacts = decode_nonempty_set(
            &artifacts_path,
            obj.take("artifacts")?,
            6,
            decode_artifact,
            |a, b| a.platform.as_ref().cmp(b.platform.as_ref()),
        )?;
        obj.finish()?;
        Ok(Self {
            digest,
            engine_version,
            build_source,
            dependency_lock,
            dependency_lock_digest,
            artifacts,
        })
    }

    /// The one artifact matching both the selected platform and name, per
    /// the manifest's no-repeated-platform law.
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

impl ReleaseArtifact {
    /// The single `executable` row, which the closure law requires to name
    /// `tree_path` with mode `100755` and the artifact's own checksum.
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

fn decode_version(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let (core, pre) = raw
        .split_once('-')
        .map_or((raw.as_str(), None), |(core, pre)| (core, Some(pre)));
    let numeric: Vec<&str> = core.split('.').collect();
    let shaped = raw.len() <= 64
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
        });
    if shaped {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_build_source(path: &str, value: Value) -> Result<BuildSource, Error> {
    let mut obj = Obj::new(path, value)?;
    let repository = obj.required("repository", crate::controls::decode_repository)?;
    let format_path = obj.field("object_format");
    let object_format = decode_enum(&format_path, obj.take("object_format")?)?;
    let commit_path = obj.field("commit_oid");
    let commit_oid = Oid::new(
        object_format,
        de::string(&commit_path, obj.take("commit_oid")?)?,
    )
    .ok_or_else(|| Error::new(&commit_path, ErrorKind::InvalidValue))?;
    obj.finish()?;
    Ok(BuildSource {
        repository,
        object_format,
        commit_oid,
    })
}

fn decode_lock(path: &str, value: Value) -> Result<DependencyLockInput, Error> {
    let mut obj = Obj::new(path, value)?;
    obj.required("schema", |path, value| {
        de::const_str(path, value, DEPENDENCY_LOCK_SCHEMA)
    })?;
    let files_path = obj.field("files");
    let rows = de::array(&files_path, obj.take("files")?)?;
    obj.finish()?;
    if rows.is_empty() || rows.len() > 32 {
        return fail(&files_path, ErrorKind::LimitExceeded);
    }
    let mut files: Vec<(RepoPathText, Digest)> = Vec::with_capacity(rows.len());
    for (index, row) in rows.into_iter().enumerate() {
        let row_path = format!("{files_path}[{index}]");
        let mut file = Obj::new(&row_path, row)?;
        let member = file.required("path", decode_repo_path)?;
        let raw_digest = file.required("raw_digest", de::digest)?;
        file.finish()?;
        files.push((member, raw_digest));
    }
    sorted_unique(&files_path, &files, |a, b| a.0.as_str().cmp(b.0.as_str()))?;
    Ok(DependencyLockInput { files })
}

fn decode_artifact(path: &str, value: Value) -> Result<ReleaseArtifact, Error> {
    let mut obj = Obj::new(path, value)?;
    let platform = obj.required("platform", decode_enum)?;
    let artifact_name = obj.required("artifact_name", decode_artifact_id)?;
    let tree_path = obj.required("tree_path", decode_repo_path)?;
    let binary_sha256 = obj.required("binary_sha256", de::digest)?;
    let engine_digest = obj.required("engine_digest", de::digest)?;
    obj.required("runtime_contract", |path, value| {
        de::const_str(path, value, RUNTIME_CONTRACT)
    })?;
    obj.required("environment_contract", |path, value| {
        de::const_str(path, value, ENVIRONMENT_CONTRACT)
    })?;
    let files_path = obj.field("runtime_files");
    let runtime_files = decode_nonempty_set(
        &files_path,
        obj.take("runtime_files")?,
        256,
        decode_runtime_file,
        |a, b| a.path.as_str().cmp(b.path.as_str()),
    )?;
    obj.finish()?;
    let artifact = ReleaseArtifact {
        platform,
        artifact_name,
        tree_path,
        binary_sha256,
        engine_digest,
        runtime_files,
    };
    if artifact.executable().is_none() {
        return fail(&files_path, ErrorKind::Inconsistent);
    }
    Ok(artifact)
}

fn decode_runtime_file(path: &str, value: Value) -> Result<RuntimeFile, Error> {
    let mut file = Obj::new(path, value)?;
    let member = file.required("path", decode_repo_path)?;
    let role = file.required("role", decode_enum)?;
    let mode_path = file.field("git_mode");
    let git_mode = match de::string(&mode_path, file.take("git_mode")?)?.as_str() {
        "100644" => GitMode::RegularFile,
        "100755" => GitMode::ExecutableFile,
        _ => return fail(&mode_path, ErrorKind::InvalidValue),
    };
    let file_sha256 = file.required("file_sha256", de::digest)?;
    file.finish()?;
    Ok(RuntimeFile {
        path: member,
        role,
        git_mode,
        file_sha256,
    })
}

fn decode_nonempty_set<T>(
    path: &str,
    value: Value,
    maximum: usize,
    decode: impl Fn(&str, Value) -> Result<T, Error>,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<Vec<T>, Error> {
    let rows = de::array(path, value)?;
    if rows.is_empty() || rows.len() > maximum {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let items = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| decode(&format!("{path}[{index}]"), row))
        .collect::<Result<Vec<_>, _>>()?;
    sorted_unique(path, &items, compare)?;
    Ok(items)
}

fn sorted_unique<T>(
    path: &str,
    items: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), Error> {
    for pair in items.windows(2) {
        if let [left, right] = pair
            && compare(left, right) != Ordering::Less
        {
            return fail(path, ErrorKind::UnsortedSet);
        }
    }
    Ok(())
}

fn decode_repo_path(path: &str, value: Value) -> Result<RepoPathText, Error> {
    RepoPathText::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_artifact_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}
