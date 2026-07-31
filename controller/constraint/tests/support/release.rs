use std::fs;
use std::path::Path;

use amiss_bootstrap::build::{
    RELEASE_MANIFEST_DIGEST_PATH, RELEASE_MANIFEST_PATH, StagedArtifact, StagedBuild, StagedFile,
    build_manifest,
};
use amiss_wire::action::host_platform;
use amiss_wire::manifest::RuntimeRole;
use tempfile::TempDir;

const ACTION: &[u8] = include_bytes!("../../../../crates/amiss/action/runtime.yml");

pub(crate) struct Release {
    pub(crate) dir: TempDir,
    pub(crate) commit: String,
    pub(crate) tree: String,
}

pub(crate) fn release(mutate: impl FnOnce(&Path)) -> Release {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    amiss_fixtures::git(root, &["init", "-q"]).unwrap();

    let platform = host_platform().unwrap();
    let binary = amiss_fixtures::executable_bytes(platform);
    let lock = b"# Cargo.lock fixture\nversion = 4\n";
    let binary_path = format!("dist/amiss-{}", platform.as_str());
    let mut artifacts = [StagedArtifact {
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
        locks: vec![("Cargo.lock".to_owned(), lock)],
    };
    let (manifest, digest) = build_manifest(&build, &mut artifacts).unwrap();

    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("action.yml"), ACTION).unwrap();
    fs::write(root.join(RELEASE_MANIFEST_PATH), manifest).unwrap();
    fs::write(
        root.join(RELEASE_MANIFEST_DIGEST_PATH),
        format!("{digest}\n"),
    )
    .unwrap();
    fs::write(root.join(&binary_path), binary).unwrap();
    fs::write(root.join("Cargo.lock"), lock).unwrap();
    mutate(root);

    amiss_fixtures::git(root, &["add", "-A"]).unwrap();
    amiss_fixtures::git(root, &["update-index", "--chmod=+x", "--", &binary_path]).unwrap();
    amiss_fixtures::git(root, &["commit", "-qm", "release"]).unwrap();
    Release {
        commit: amiss_fixtures::git(root, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_owned(),
        tree: amiss_fixtures::git(root, &["rev-parse", "HEAD^{tree}"])
            .unwrap()
            .trim()
            .to_owned(),
        dir,
    }
}
