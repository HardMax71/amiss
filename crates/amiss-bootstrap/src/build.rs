use amiss_wire::controls::{ConstraintPlatform, GitMode};
use amiss_wire::manifest::{
    BuildSource, DependencyLockFile, DependencyLockInput, DependencyLockSchema,
    EnvironmentContract, ReleaseArtifact, ReleaseManifest, ReleaseManifestSchema, RuntimeContract,
    RuntimeFile, RuntimeRole,
};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};
use amiss_wire::model::{Digest, RAW_EVIDENCE_DOMAIN};
use sha2::Digest as _;

use crate::ENGINE_DOMAIN;

pub const RELEASE_MANIFEST_PATH: &str = "release-manifest.json";
pub const RELEASE_MANIFEST_DIGEST_PATH: &str = "release-manifest.digest";

/// One staged runtime file: its action-tree path, its role, whether Git will
/// record the execute bit, and its exact bytes.
pub struct StagedFile<'bytes> {
    pub path: String,
    pub role: RuntimeRole,
    pub executable: bool,
    pub bytes: &'bytes [u8],
}

/// One staged platform artifact: the closed platform row, the published
/// artifact name, and its complete runtime closure. Exactly one file must
/// carry the `executable` role.
pub struct StagedArtifact<'bytes> {
    pub platform: ConstraintPlatform,
    pub artifact_name: String,
    pub files: Vec<StagedFile<'bytes>>,
}

/// The build namespace and the lockfiles that pinned it.
pub struct StagedBuild<'bytes> {
    pub engine_version: String,
    pub host: String,
    pub owner: String,
    pub repository: String,
    pub object_format: &'static str,
    pub commit_oid: String,
    pub locks: Vec<(String, &'bytes [u8])>,
}

/// Builds the strict release manifest from the staged action tree: every
/// digest is computed from the exact staged bytes, and every set is sorted
/// before the typed wire model validates and serializes it.
///
/// # Errors
///
/// The staged release cannot form a valid release-manifest contract.
pub fn build_manifest(
    build: &StagedBuild<'_>,
    artifacts: &mut [StagedArtifact<'_>],
) -> Result<(Vec<u8>, Digest), &'static str> {
    let mut files = build
        .locks
        .iter()
        .map(|(path, bytes)| {
            Ok(DependencyLockFile {
                path: RepoPathText::new(path.clone()).ok_or("invalid dependency lock path")?,
                raw_digest: Digest::from(
                    sha2::Sha256::new_with_prefix(RAW_EVIDENCE_DOMAIN)
                        .chain_update([0_u8])
                        .chain_update(bytes)
                        .finalize()
                        .0,
                ),
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    files.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
    let dependency_lock = DependencyLockInput {
        schema: DependencyLockSchema::Current,
        files,
    };
    dependency_lock
        .validate()
        .map_err(|_defect| "invalid dependency lock")?;
    let mut writer = digest_io::IoWrapper(
        sha2::Sha256::new_with_prefix(amiss_wire::manifest::DEPENDENCY_LOCK_DOMAIN)
            .chain_update([0_u8]),
    );
    serde_json_canonicalizer::to_writer(&dependency_lock, &mut writer)
        .map_err(|_defect| "invalid dependency lock")?;
    let dependency_lock_digest = Digest::from(writer.0.finalize().0);

    artifacts.sort_by(|left, right| left.platform.as_ref().cmp(right.platform.as_ref()));
    let artifacts = artifacts
        .iter_mut()
        .map(build_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    let object_format = build
        .object_format
        .parse::<ObjectFormat>()
        .map_err(|_defect| "invalid build object format")?;
    let manifest = ReleaseManifest {
        schema: ReleaseManifestSchema::Current,
        engine_version: build.engine_version.clone(),
        build_source: BuildSource {
            repository: RepositoryIdentity::new(
                build.host.clone(),
                build.owner.clone(),
                build.repository.clone(),
            )
            .ok_or("invalid build repository")?,
            object_format,
            commit_oid: Oid::new(object_format, build.commit_oid.clone())
                .ok_or("invalid build commit")?,
        },
        dependency_lock,
        dependency_lock_digest,
        artifacts,
    };
    manifest
        .validate()
        .map_err(|_defect| "invalid release manifest")?;
    let mut bytes = serde_json_canonicalizer::to_vec(&manifest)
        .map_err(|_defect| "invalid release manifest")?;
    let digest = Digest::from(
        sha2::Sha256::new_with_prefix(amiss_wire::manifest::MANIFEST_DOMAIN)
            .chain_update([0_u8])
            .chain_update(&bytes)
            .finalize()
            .0,
    );
    bytes.push(b'\n');
    Ok((bytes, digest))
}

fn build_artifact(artifact: &mut StagedArtifact<'_>) -> Result<ReleaseArtifact, &'static str> {
    artifact
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    let mut executables = artifact
        .files
        .iter()
        .filter(|file| file.role == RuntimeRole::Executable);
    let engine = executables.next().ok_or("no executable row")?;
    if executables.next().is_some() {
        return Err("more than one executable row");
    }
    if !engine.executable {
        return Err("the executable row is not mode 100755");
    }
    let binary_sha256 = Digest::from(sha2::Sha256::digest(engine.bytes).0);
    let runtime_files = artifact
        .files
        .iter()
        .map(|file| {
            Ok(RuntimeFile {
                path: RepoPathText::new(file.path.clone()).ok_or("invalid runtime path")?,
                role: file.role,
                git_mode: if file.executable {
                    GitMode::ExecutableFile
                } else {
                    GitMode::RegularFile
                },
                file_sha256: Digest::from(sha2::Sha256::digest(file.bytes).0),
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    Ok(ReleaseArtifact {
        platform: artifact.platform,
        artifact_name: ArtifactId::new(artifact.artifact_name.clone())
            .ok_or("invalid artifact name")?,
        tree_path: RepoPathText::new(engine.path.clone()).ok_or("invalid executable path")?,
        binary_sha256,
        engine_digest: Digest::from(
            sha2::Sha256::new_with_prefix(ENGINE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(engine.bytes)
                .finalize()
                .0,
        ),
        runtime_contract: RuntimeContract::Current,
        environment_contract: EnvironmentContract::Current,
        runtime_files,
    })
}
