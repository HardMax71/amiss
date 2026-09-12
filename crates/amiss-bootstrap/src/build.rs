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
    pub path: RepoPathText,
    pub role: RuntimeRole,
    pub executable: bool,
    pub bytes: &'bytes [u8],
}

/// One staged platform artifact: the closed platform row, the published
/// artifact name, and its complete runtime closure. Exactly one file must
/// carry the `executable` role.
pub struct StagedArtifact<'bytes> {
    pub platform: ConstraintPlatform,
    pub artifact_name: ArtifactId,
    pub files: Vec<StagedFile<'bytes>>,
}

/// The build namespace and the lockfiles that pinned it.
pub struct StagedBuild<'bytes> {
    pub engine_version: String,
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    pub commit_oid: Oid,
    pub locks: Vec<(RepoPathText, &'bytes [u8])>,
}

/// Builds the strict release manifest from the staged action tree: every
/// digest is computed from the exact staged bytes, and every set is sorted
/// before the typed wire model validates and serializes it.
///
/// # Errors
///
/// The staged release cannot form a valid release-manifest contract.
pub fn build_manifest<'bytes>(
    build: StagedBuild<'_>,
    artifacts: impl IntoIterator<Item = StagedArtifact<'bytes>>,
) -> Result<(Vec<u8>, Digest), &'static str> {
    let mut files = build
        .locks
        .into_iter()
        .map(|(path, bytes)| DependencyLockFile {
            path,
            raw_digest: Digest::from(
                sha2::Sha256::new_with_prefix(RAW_EVIDENCE_DOMAIN)
                    .chain_update([0_u8])
                    .chain_update(bytes)
                    .finalize()
                    .0,
            ),
        })
        .collect::<Vec<_>>();
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

    let mut artifacts = artifacts
        .into_iter()
        .map(build_artifact)
        .collect::<Result<Vec<_>, _>>()?;
    artifacts.sort_by(|left, right| left.platform.as_ref().cmp(right.platform.as_ref()));
    let manifest = ReleaseManifest {
        schema: ReleaseManifestSchema::Current,
        engine_version: build.engine_version,
        build_source: BuildSource {
            repository: build.repository,
            object_format: build.object_format,
            commit_oid: build.commit_oid,
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

fn build_artifact(mut artifact: StagedArtifact<'_>) -> Result<ReleaseArtifact, &'static str> {
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
    let tree_path = engine.path.clone();
    let engine_digest = Digest::from(
        sha2::Sha256::new_with_prefix(ENGINE_DOMAIN)
            .chain_update([0_u8])
            .chain_update(engine.bytes)
            .finalize()
            .0,
    );
    let runtime_files = artifact
        .files
        .into_iter()
        .map(|file| RuntimeFile {
            path: file.path,
            role: file.role,
            git_mode: if file.executable {
                GitMode::ExecutableFile
            } else {
                GitMode::RegularFile
            },
            file_sha256: Digest::from(sha2::Sha256::digest(file.bytes).0),
        })
        .collect::<Vec<_>>();
    Ok(ReleaseArtifact {
        platform: artifact.platform,
        artifact_name: artifact.artifact_name,
        tree_path,
        binary_sha256,
        engine_digest,
        runtime_contract: RuntimeContract::Current,
        environment_contract: EnvironmentContract::Current,
        runtime_files,
    })
}
