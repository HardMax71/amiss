use amiss_wire::controls::ConstraintPlatform;
use amiss_wire::digest::{Digest, hb, hj, sha256};
use amiss_wire::json::{Value, canonical};
use amiss_wire::manifest::{
    DEPENDENCY_LOCK_DOMAIN, DEPENDENCY_LOCK_SCHEMA, ENVIRONMENT_CONTRACT, MANIFEST_DOMAIN,
    MANIFEST_SCHEMA, RUNTIME_CONTRACT, RuntimeRole,
};

use crate::ENGINE_DOMAIN;

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
    pub owner: String,
    pub repository: String,
    pub object_format: &'static str,
    pub commit_oid: String,
    pub locks: Vec<(String, &'bytes [u8])>,
}

/// Builds the strict release manifest from the staged action tree: every
/// digest is computed from the exact staged bytes, artifacts sort by
/// platform, runtime files and lockfiles sort by path, and the lock-set
/// digest is `HJ` over the complete lock object. Returns the manifest bytes
/// (`JCS || LF`, the blob the action tree carries) and the semantic digest
/// the execution constraint pins.
///
/// # Errors
///
/// A staged artifact without exactly one executable row, which is a
/// malformed release rather than a runtime condition.
pub fn build_manifest(
    build: &StagedBuild<'_>,
    artifacts: &mut [StagedArtifact<'_>],
) -> Result<(Vec<u8>, Digest), &'static str> {
    let mut locks: Vec<(String, Digest)> = build
        .locks
        .iter()
        .map(|(path, bytes)| (path.clone(), hb("amiss/raw-evidence/v1", bytes)))
        .collect();
    locks.sort_by(|a, b| a.0.cmp(&b.0));
    let lock_value = object(vec![
        ("schema", string(DEPENDENCY_LOCK_SCHEMA)),
        (
            "files",
            Value::Array(
                locks
                    .iter()
                    .map(|(path, digest)| {
                        object(vec![
                            ("path", string(path)),
                            ("raw_digest", string(&digest.to_string())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let lock_digest = hj(DEPENDENCY_LOCK_DOMAIN, &lock_value);

    artifacts.sort_by_key(|artifact| artifact.platform.as_str());
    let mut rows: Vec<Value> = Vec::with_capacity(artifacts.len());
    for artifact in artifacts.iter_mut() {
        rows.push(artifact_value(artifact)?);
    }

    let manifest = object(vec![
        ("schema", string(MANIFEST_SCHEMA)),
        ("engine_version", string(&build.engine_version)),
        (
            "build_source",
            object(vec![
                (
                    "repository",
                    object(vec![
                        ("host", string("github.com")),
                        ("owner", string(&build.owner)),
                        ("name", string(&build.repository)),
                    ]),
                ),
                ("object_format", string(build.object_format)),
                ("commit_oid", string(&build.commit_oid)),
            ]),
        ),
        ("dependency_lock", lock_value),
        ("dependency_lock_digest", string(&lock_digest.to_string())),
        ("artifacts", Value::Array(rows)),
    ]);
    let digest = hj(MANIFEST_DOMAIN, &manifest);
    let mut bytes = canonical(&manifest);
    bytes.push(b'\n');
    Ok((bytes, digest))
}

fn artifact_value(artifact: &mut StagedArtifact<'_>) -> Result<Value, &'static str> {
    artifact.files.sort_by(|a, b| a.path.cmp(&b.path));
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
    let mut launchers = artifact
        .files
        .iter()
        .filter(|file| file.role == RuntimeRole::Launcher);
    let launcher = launchers.next().ok_or("no launcher row")?;
    if launchers.next().is_some() {
        return Err("more than one launcher row");
    }
    if launcher.executable {
        return Err("the launcher row is not mode 100644");
    }
    let binary_sha256 = sha256(engine.bytes);
    let engine_digest = hb(ENGINE_DOMAIN, engine.bytes);
    let tree_path = engine.path.clone();

    let files: Vec<Value> = artifact
        .files
        .iter()
        .map(|file| {
            object(vec![
                ("path", string(&file.path)),
                ("role", string(file.role.as_str())),
                (
                    "git_mode",
                    string(if file.executable { "100755" } else { "100644" }),
                ),
                ("file_sha256", string(&sha256(file.bytes).to_string())),
            ])
        })
        .collect();

    Ok(object(vec![
        ("platform", string(artifact.platform.as_str())),
        ("artifact_name", string(&artifact.artifact_name)),
        ("tree_path", string(&tree_path)),
        ("binary_sha256", string(&binary_sha256.to_string())),
        ("engine_digest", string(&engine_digest.to_string())),
        ("runtime_contract", string(RUNTIME_CONTRACT)),
        ("environment_contract", string(ENVIRONMENT_CONTRACT)),
        ("runtime_files", Value::Array(files)),
    ]))
}

/// The action tree's root `action.yml`: JCS JSON plus LF, exactly `name`,
/// `description`, and `runs`. The declared launcher is manifest-listed and
/// exists for experimental convenience; the required path never executes it.
#[must_use]
pub fn action_metadata(name: &str, description: &str, main: &str) -> Vec<u8> {
    let metadata = object(vec![
        ("name", string(name)),
        ("description", string(description)),
        (
            "runs",
            object(vec![("main", string(main)), ("using", string("node20"))]),
        ),
    ]);
    let mut bytes = canonical(&metadata);
    bytes.push(b'\n');
    bytes
}

fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
