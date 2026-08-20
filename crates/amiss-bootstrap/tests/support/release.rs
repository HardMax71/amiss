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
    release_with_engine(&amiss_fixtures::executable_bytes(platform), mutate)
}

pub(crate) fn release_with_engine(engine: &[u8], mutate: impl FnOnce(&Path)) -> Release {
    let platform = host_platform().expect("a supported test platform");
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    amiss_fixtures::init_repository(root).expect("initialize repository");

    let binary = engine.to_vec();
    let lock = b"# Cargo.lock fixture\nversion = 4\n".to_vec();
    let binary_path = format!("dist/amiss-{}", platform.as_ref());
    let mut artifacts = vec![StagedArtifact {
        platform,
        artifact_name: format!("amiss-{}", platform.as_ref()),
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
    fs::write(root.join(&binary_path), &binary).unwrap();
    fs::write(root.join("Cargo.lock"), &lock).unwrap();
    mutate(root);

    let committed =
        amiss_fixtures::commit_worktree(root, &[&binary_path], "release").expect("commit release");
    Release {
        dir,
        commit: committed.id,
        tree: committed.tree,
        manifest_digest,
        engine_digest,
        platform,
    }
}
