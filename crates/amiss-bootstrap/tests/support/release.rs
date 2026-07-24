use std::fs;
use std::path::Path;

use amiss_bootstrap::build::{
    RELEASE_MANIFEST_DIGEST_PATH, RELEASE_MANIFEST_PATH, StagedArtifact, StagedBuild, StagedFile,
    build_manifest,
};
use amiss_wire::action::host_platform;
use amiss_wire::controls::ConstraintPlatform;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::manifest::RuntimeRole;
use tempfile::TempDir;

pub(crate) use amiss_fixtures::{executable_bytes as engine_bytes, git};

pub(crate) const LAUNCHER: &[u8] = include_bytes!("../../../amiss/action/launcher.js");
pub(crate) const ACTION: &[u8] = include_bytes!("../../../amiss/action/runtime.yml");

pub(crate) struct Release {
    pub dir: TempDir,
    pub commit: String,
    pub tree: String,
    pub manifest_digest: Digest,
    pub engine_digest: Digest,
    pub platform: ConstraintPlatform,
}

pub(crate) fn release(mutate: impl FnOnce(&Path)) -> Release {
    let platform = host_platform().expect("a supported test platform");
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]).expect("initialize repository");

    let binary = amiss_fixtures::executable_bytes(platform);
    let launcher = LAUNCHER.to_vec();
    let lock = b"# Cargo.lock fixture\nversion = 4\n".to_vec();
    let binary_path = format!("dist/amiss-{}", platform.as_str());
    let mut artifacts = vec![StagedArtifact {
        platform,
        artifact_name: format!("amiss-{}", platform.as_str()),
        files: vec![
            StagedFile {
                path: binary_path.clone(),
                role: RuntimeRole::Executable,
                executable: true,
                bytes: &binary,
            },
            StagedFile {
                path: "dist/launcher.js".to_owned(),
                role: RuntimeRole::Launcher,
                executable: false,
                bytes: &launcher,
            },
            StagedFile {
                path: "action.yml".to_owned(),
                role: RuntimeRole::RuntimeData,
                executable: false,
                bytes: ACTION,
            },
        ],
    }];
    let build = StagedBuild {
        engine_version: "0.1.0-experimental".to_owned(),
        host: "git.example.internal".to_owned(),
        owner: "platform/security".to_owned(),
        repository: "amiss".to_owned(),
        object_format: "sha1",
        commit_oid: "a".repeat(40),
        locks: vec![("Cargo.lock".to_owned(), &lock)],
    };
    let (manifest_bytes, manifest_digest) = build_manifest(&build, &mut artifacts).unwrap();
    let engine_digest = hb(amiss_bootstrap::ENGINE_DOMAIN, &binary);

    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("action.yml"), ACTION).unwrap();
    fs::write(root.join(RELEASE_MANIFEST_PATH), &manifest_bytes).unwrap();
    fs::write(
        root.join(RELEASE_MANIFEST_DIGEST_PATH),
        format!("{manifest_digest}\n"),
    )
    .unwrap();
    fs::write(root.join("dist/launcher.js"), &launcher).unwrap();
    fs::write(root.join(&binary_path), &binary).unwrap();
    fs::write(root.join("Cargo.lock"), &lock).unwrap();
    mutate(root);

    git(root, &["add", "-A"]).expect("stage release");
    executable(root, &binary_path);
    git(root, &["commit", "-qm", "release"]).expect("commit release");
    let commit = git(root, &["rev-parse", "HEAD"])
        .expect("resolve release commit")
        .trim()
        .to_owned();
    let tree = git(root, &["rev-parse", "HEAD^{tree}"])
        .expect("resolve release tree")
        .trim()
        .to_owned();
    Release {
        dir,
        commit,
        tree,
        manifest_digest,
        engine_digest,
        platform,
    }
}

pub(crate) fn executable(root: &Path, path: &str) {
    if fs::symlink_metadata(root.join(path)).is_ok_and(|entry| entry.is_file()) {
        git(root, &["update-index", "--chmod=+x", "--", path]).expect("mark executable in index");
    }
}
