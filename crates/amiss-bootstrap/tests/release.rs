#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_bootstrap::build::{StagedArtifact, StagedBuild, StagedFile, build_manifest};
use amiss_bootstrap::{Refusal, validate};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::action::host_platform;
use amiss_wire::controls::{ConstraintPlatform, ExecutionConstraintDescriptor};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::{Value, canonical};
use amiss_wire::manifest::{ReleaseManifest, RuntimeRole};
use amiss_wire::model::ObjectFormat;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).expect("run git")
}

/// A plausible engine binary for the running platform: the real header bytes
/// the bootstrap derives the target from, padded with body content so the
/// digests are not degenerate.
fn engine_bytes(platform: ConstraintPlatform) -> Vec<u8> {
    let mut bytes = match platform {
        ConstraintPlatform::LinuxX8664 | ConstraintPlatform::LinuxAarch64 => {
            let machine: [u8; 2] = if platform == ConstraintPlatform::LinuxX8664 {
                [0x3e, 0x00]
            } else {
                [0xb7, 0x00]
            };
            let mut header = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
            header.extend_from_slice(&[0; 8]);
            header.extend_from_slice(&[0x02, 0x00]);
            header.extend_from_slice(&machine);
            header
        }
        ConstraintPlatform::MacosX8664 | ConstraintPlatform::MacosAarch64 => {
            let cpu: [u8; 4] = if platform == ConstraintPlatform::MacosX8664 {
                [0x07, 0x00, 0x00, 0x01]
            } else {
                [0x0c, 0x00, 0x00, 0x01]
            };
            let mut header = vec![0xcf, 0xfa, 0xed, 0xfe];
            header.extend_from_slice(&cpu);
            header
        }
        ConstraintPlatform::WindowsX8664 | ConstraintPlatform::WindowsAarch64 => {
            let machine: [u8; 2] = if platform == ConstraintPlatform::WindowsX8664 {
                [0x64, 0x86]
            } else {
                [0x64, 0xaa]
            };
            let mut header = vec![b'M', b'Z'];
            header.resize(0x3c, 0);
            header.extend_from_slice(&0x40_u32.to_le_bytes());
            header.extend_from_slice(b"PE\0\0");
            header.extend_from_slice(&machine);
            header
        }
    };
    bytes.extend_from_slice(&[0x90; 512]);
    bytes
}

/// The launcher this repository ships, so the closure these tests validate is
/// the closure a release publishes, byte for byte.
const LAUNCHER: &[u8] = include_bytes!("../../amiss/action/launcher.js");

const ACTION: &[u8] = include_bytes!("../../amiss/action/runtime.yml");

struct Release {
    dir: TempDir,
    commit: String,
    tree: String,
    manifest_digest: Digest,
    engine_digest: Digest,
    platform: ConstraintPlatform,
}

/// Stages a real action tree: the canonical `action.yml`, the generated
/// manifest, the launcher, and the platform binary, committed to a git
/// repository exactly as a release would publish it.
fn release(mutate: impl FnOnce(&Path)) -> Release {
    let platform = host_platform().expect("a supported test platform");
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);

    let binary = engine_bytes(platform);
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
    fs::write(root.join("release-manifest.json"), &manifest_bytes).unwrap();
    fs::write(root.join("dist/launcher.js"), &launcher).unwrap();
    fs::write(root.join(&binary_path), &binary).unwrap();
    fs::write(root.join("Cargo.lock"), &lock).unwrap();
    mutate(root);

    git(root, &["add", "-A"]);
    executable(root, &binary_path);
    git(root, &["commit", "-qm", "release"]);
    let commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    Release {
        dir,
        commit,
        tree,
        manifest_digest,
        engine_digest,
        platform,
    }
}

/// The engine's tree entry must be mode 100755. The bit is set on the index
/// entry, after staging: unix carries it in the worktree, which `git add`
/// reads and would otherwise overwrite, and Windows has no such bit at all. A
/// mutation that replaces the engine with something other than a regular file
/// has no executable bit to set, and says so by leaving the entry alone.
fn executable(root: &Path, path: &str) {
    if fs::symlink_metadata(root.join(path)).is_ok_and(|entry| entry.is_file()) {
        git(root, &["update-index", "--chmod=+x", "--", path]);
    }
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

/// The execution constraint the required workflow protects, pinning this
/// exact action commit, tree, manifest digest, platform, and bootstrap.
fn constraint(release: &Release) -> ExecutionConstraintDescriptor {
    let value = object(vec![
        ("schema", string("amiss/scanner-execution-constraint")),
        (
            "action_repository",
            object(vec![
                ("host", string("git.example.internal")),
                ("owner", string("platform/security")),
                ("name", string("amiss")),
            ]),
        ),
        ("action_object_format", string("sha1")),
        ("action_commit_oid", string(&release.commit)),
        ("action_tree_oid", string(&release.tree)),
        ("manifest_path", string("release-manifest.json")),
        (
            "release_manifest_digest",
            string(&release.manifest_digest.to_string()),
        ),
        ("selected_platform", string(release.platform.as_str())),
        ("required_status_name", string("amiss / assure")),
        ("bootstrap_contract", string("amiss-action-bootstrap")),
        (
            "bootstrap_digest",
            string(&hb(amiss_bootstrap::BOOTSTRAP_DOMAIN, BOOTSTRAP).to_string()),
        ),
    ]);
    ExecutionConstraintDescriptor::parse(&canonical(&value)).expect("the constraint parses")
}

fn attempt(release: &Release, bootstrap: &[u8]) -> Result<amiss_bootstrap::Validated, Refusal> {
    let repo = Repository::open(release.dir.path(), ObjectFormat::Sha1).expect("open action tree");
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    validate(&repo, &mut resources, &constraint(release), bootstrap)
}

const BOOTSTRAP: &[u8] = b"the exact protected bootstrap bytes";

#[test]
fn the_pinned_release_validates_end_to_end() {
    let release = release(|_root| {});
    let validated = attempt(&release, BOOTSTRAP).expect("the staged release validates");
    assert_eq!(validated.platform, release.platform);
    assert_eq!(validated.engine_digest, release.engine_digest);
    assert_eq!(validated.manifest.engine_version, "0.1.0-experimental");
    assert_eq!(
        validated.manifest.build_source.repository.owner,
        "platform/security"
    );
    assert_eq!(validated.artifact.runtime_files.len(), 3);
    assert!(
        validated.artifact.runtime_files.iter().any(|file| {
            file.role == RuntimeRole::RuntimeData && file.path.as_str() == "action.yml"
        }),
        "the runnable action definition is a pinned closure row"
    );
    assert_eq!(
        validated.manifest.dependency_lock.files.len(),
        1,
        "the lock set carries every build lockfile"
    );
}

#[test]
fn the_generated_manifest_reparses_to_its_pinned_digest() {
    let release = release(|_root| {});
    let bytes = fs::read(release.dir.path().join("release-manifest.json")).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'), "the manifest blob ends in LF");
    let parsed = ReleaseManifest::parse(&bytes).expect("the generated manifest parses");
    assert_eq!(parsed.digest, release.manifest_digest);
    assert_eq!(
        canonical(&amiss_wire::json::parse(bytes.strip_suffix(b"\n").unwrap()).unwrap()),
        bytes.strip_suffix(b"\n").unwrap(),
        "the manifest blob is exactly its own canonicalization"
    );
}

/// `runs.main` is what GitHub executes if a consumer writes
/// `uses: owner/amiss@sha`, so it is the one file in the tree that can report a
/// result without running the engine. It may fail and it may explain itself. It
/// may never exit 0, because a green check nobody earned is the failure this
/// whole project exists to refuse. Node ships on every runner image the release
/// targets, so a missing interpreter is a broken environment, not a skip.
#[test]
fn the_shipped_launcher_refuses_instead_of_passing() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../amiss/action/launcher.js"
        ))
        .output()
        .expect("node runs the launcher; every runner image ships it");
    assert_eq!(
        output.status.code(),
        Some(2),
        "the launcher checked nothing, so it owes exit 2"
    );
    let said = String::from_utf8(output.stderr).expect("the launcher explains itself in UTF-8");
    assert!(
        said.contains("nothing was checked"),
        "the refusal says what did not happen: {said}"
    );
}

#[test]
fn a_bootstrap_whose_bytes_differ_refuses_before_anything_else() {
    let release = release(|_root| {});
    let outcome = attempt(&release, b"a different bootstrap binary");
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("bootstrap-digest-mismatch"))
    );
}

/// A file that is a symlink, which only a privileged Windows process can
/// create. The directory sides of the same law run on every platform, in
/// `amiss-git`'s `boundary.rs`.
#[cfg(unix)]
#[test]
fn a_symlinked_engine_path_refuses() {
    let release = release(|root| {
        let platform = host_platform().unwrap();
        let staged = root.join(format!("dist/amiss-{}", platform.as_str()));
        fs::remove_file(&staged).unwrap();
        std::os::unix::fs::symlink("../Cargo.lock", &staged).unwrap();
    });
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("path-not-regular-blob")),
        "a symlink at the artifact path is never followed"
    );
}

/// Strips every `launcher` runtime row, wherever it appears. What is left is a
/// manifest a careless or hostile release could publish: internally consistent,
/// correctly digested, and silent about the one file `runs.main` names.
fn strip_launcher_rows(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| !is_launcher_row(item));
            for item in items.iter_mut() {
                strip_launcher_rows(item);
            }
        }
        Value::Object(members) => {
            for (_key, member) in members.iter_mut() {
                strip_launcher_rows(member);
            }
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) => {}
    }
}

fn is_launcher_row(value: &Value) -> bool {
    let Value::Object(members) = value else {
        return false;
    };
    members.iter().any(|(key, member)| {
        key == "role" && matches!(member, Value::String(role) if role == "launcher")
    })
}

fn strip_action_rows(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| !is_action_row(item));
            for item in items.iter_mut() {
                strip_action_rows(item);
            }
        }
        Value::Object(members) => {
            for (_key, member) in members.iter_mut() {
                strip_action_rows(member);
            }
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) => {}
    }
}

fn is_action_row(value: &Value) -> bool {
    let Value::Object(members) = value else {
        return false;
    };
    members.iter().any(|(key, member)| {
        key == "role" && matches!(member, Value::String(role) if role == "runtime-data")
    })
}

/// The action definition is what a `uses:` workflow actually executes, so a
/// manifest whose closure fails to pin it must refuse even when its digest
/// is self-consistent: otherwise the one runnable file at the tree root is
/// the one file nothing checks.
#[test]
fn a_manifest_that_omits_the_action_row_is_refused() {
    let mut restripped: Option<Digest> = None;
    let mut release = release(|root| {
        let path = root.join("release-manifest.json");
        let bytes = fs::read(&path).unwrap();
        let mut value =
            amiss_wire::json::parse(bytes.strip_suffix(b"\n").expect("the manifest ends in LF"))
                .expect("the manifest parses");
        strip_action_rows(&mut value);
        restripped = Some(amiss_wire::digest::hj(
            amiss_wire::manifest::MANIFEST_DOMAIN,
            &value,
        ));
        let mut out = canonical(&value);
        out.push(b'\n');
        fs::write(&path, out).unwrap();
    });
    release.manifest_digest = restripped.expect("the stripped manifest was digested");

    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("action-metadata-invalid")),
        "an unpinned action definition is a refusal, not a runnable file"
    );
}

/// `runs.main` is the one file a `uses:` consumer executes, and pinning its
/// bytes is the whole reason the runtime closure exists. A manifest may not
/// stay silent about it: the row is required, exactly one, and mode `100644`.
/// Without this the manifest below validates clean while the launcher it points
/// at is never resolved in the tree at all.
#[test]
fn a_manifest_that_omits_the_launcher_row_is_refused() {
    let mut restripped: Option<Digest> = None;
    let mut release = release(|root| {
        let path = root.join("release-manifest.json");
        let bytes = fs::read(&path).unwrap();
        let mut value =
            amiss_wire::json::parse(bytes.strip_suffix(b"\n").expect("the manifest ends in LF"))
                .expect("the manifest parses");
        strip_launcher_rows(&mut value);
        restripped = Some(amiss_wire::digest::hj(
            amiss_wire::manifest::MANIFEST_DOMAIN,
            &value,
        ));
        let mut out = canonical(&value);
        out.push(b'\n');
        fs::write(&path, out).unwrap();
    });
    release.manifest_digest = restripped.expect("the stripped manifest was digested");

    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("manifest-unreadable")),
        "a manifest whose closure omits the launcher is not a manifest"
    );
}

#[test]
fn a_tampered_runtime_file_refuses_on_its_checksum() {
    let release = release(|root| {
        fs::write(root.join("dist/launcher.js"), b"// swapped after staging\n").unwrap();
    });
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("runtime-closure-mismatch"))
    );
}

#[test]
fn a_manifest_from_another_tree_refuses_on_its_digest() {
    let mut release = release(|_root| {});
    release.manifest_digest = hb("amiss/scanner-release-manifest", b"another tree");
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("manifest-digest-mismatch"))
    );
}

#[test]
fn an_engine_whose_header_names_another_platform_refuses() {
    let platform = host_platform().unwrap();
    let other = match platform {
        ConstraintPlatform::LinuxX8664
        | ConstraintPlatform::MacosX8664
        | ConstraintPlatform::MacosAarch64
        | ConstraintPlatform::WindowsX8664
        | ConstraintPlatform::WindowsAarch64 => ConstraintPlatform::LinuxAarch64,
        ConstraintPlatform::LinuxAarch64 => ConstraintPlatform::LinuxX8664,
    };
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);

    let binary = engine_bytes(other);
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
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("action.yml"), ACTION).unwrap();
    fs::write(root.join("release-manifest.json"), &manifest_bytes).unwrap();
    fs::write(root.join("dist/launcher.js"), &launcher).unwrap();
    fs::write(root.join(&binary_path), &binary).unwrap();
    fs::write(root.join("Cargo.lock"), &lock).unwrap();
    git(root, &["add", "-A"]);
    executable(root, &binary_path);
    git(root, &["commit", "-qm", "mismatched"]);

    let release = Release {
        commit: git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
        tree: git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned(),
        dir,
        manifest_digest,
        engine_digest: hb(amiss_bootstrap::ENGINE_DOMAIN, &binary),
        platform,
    };
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("platform-binding-mismatch")),
        "the target comes from the executable header, not the manifest label"
    );
}

/// The execution constraint the required workflow protects pins the exact action
/// commit and the exact tree that commit must carry. The bootstrap resolves the
/// commit and refuses unless its tree is the pinned one, which is what stops a
/// verified workflow from being pointed at a commit whose tree was swapped under
/// it. Two ways to miss: a constraint whose tree OID is not the commit's real
/// tree, and a constraint whose commit OID names no object at all. Both are one
/// refusal, `ActionTree`, because both mean the pinned action is not the action
/// on disk.
#[test]
fn a_constraint_whose_commit_or_tree_does_not_match_refuses_on_the_action_tree() {
    let mut with_wrong_tree = release(|_root| {});
    assert_ne!(
        with_wrong_tree.tree,
        "b".repeat(40),
        "the bogus tree is not the real one"
    );
    with_wrong_tree.tree = "b".repeat(40);
    assert_eq!(
        attempt(&with_wrong_tree, BOOTSTRAP).err(),
        Some(Refusal::Tampered("action-tree-mismatch")),
        "the commit is real, but its tree is not the one the constraint pinned"
    );

    let mut with_absent_commit = release(|_root| {});
    with_absent_commit.commit = "c".repeat(40);
    assert_eq!(
        attempt(&with_absent_commit, BOOTSTRAP).err(),
        Some(Refusal::Tampered("action-tree-mismatch")),
        "the constraint pins a commit the action repository does not hold"
    );
}

/// The manifest records every build lockfile by path and raw-byte digest, and
/// its parse binds that set to the set digest, so the recorded numbers cannot
/// disagree with each other. What nothing checked until now is the tree: the
/// shipped Cargo.lock could carry any bytes at all, and validation would echo
/// the manifest's story about it. The lockfile is not executed, but it is the
/// one file that says which dependencies built the engine, so a release whose
/// lock bytes drifted from their recorded digest refuses instead of validating.
#[test]
fn a_tampered_lockfile_refuses_on_its_recorded_digest() {
    let release = release(|root| {
        fs::write(
            root.join("Cargo.lock"),
            b"# a different lock\nversion = 4\n",
        )
        .unwrap();
    });
    assert_eq!(
        attempt(&release, BOOTSTRAP).err(),
        Some(Refusal::Tampered("dependency-lock-mismatch")),
        "the tree's lock bytes do not recompute to the manifest's digest"
    );
}

/// The absent twin: a release tree that dropped the lockfile entirely. The
/// path comes from the manifest, the resolution walks the pinned tree, and an
/// entry that is not there is the same refusal as any other path the closure
/// names and the tree cannot produce.
#[test]
fn a_release_missing_its_lockfile_refuses_on_the_path() {
    let release = release(|root| {
        fs::remove_file(root.join("Cargo.lock")).unwrap();
    });
    assert_eq!(
        attempt(&release, BOOTSTRAP).err(),
        Some(Refusal::Tampered("path-not-regular-blob")),
        "a lockfile the manifest records and the tree lacks is not a lockfile"
    );
}
