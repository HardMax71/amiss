#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_bootstrap::build::{
    StagedArtifact, StagedBuild, StagedFile, action_metadata, build_manifest,
};
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
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output")
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
    let launcher = b"// experimental launcher, never executed on the required path\n".to_vec();
    let lock = b"# Cargo.lock fixture\nversion = 4\n".to_vec();
    let binary_path = format!("dist/amiss-{}", platform.as_str());

    let metadata = action_metadata(
        "Amiss",
        "Documentation assurance for pull requests.",
        "dist/launcher.js",
    );

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
        ],
    }];
    let build = StagedBuild {
        engine_version: "0.1.0-experimental".to_owned(),
        owner: "hardmax71".to_owned(),
        repository: "amiss".to_owned(),
        object_format: "sha1",
        commit_oid: "a".repeat(40),
        locks: vec![("Cargo.lock".to_owned(), &lock)],
    };
    let (manifest_bytes, manifest_digest) = build_manifest(&build, &mut artifacts).unwrap();
    let engine_digest = hb(amiss_bootstrap::ENGINE_DOMAIN, &binary);

    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("action.yml"), &metadata).unwrap();
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
        ("schema", string("amiss/scanner-execution-constraint/v1")),
        (
            "action_repository",
            object(vec![
                ("host", string("github.com")),
                ("owner", string("hardmax71")),
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
        ("bootstrap_contract", string("amiss-action-bootstrap-v1")),
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
    assert_eq!(validated.artifact.runtime_files.len(), 2);
    assert_eq!(validated.metadata.main.as_str(), "dist/launcher.js");
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

#[test]
fn a_bootstrap_whose_bytes_differ_refuses_before_anything_else() {
    let release = release(|_root| {});
    let outcome = attempt(&release, b"a different bootstrap binary");
    assert_eq!(outcome.err(), Some(Refusal::BootstrapDigest));
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
        Some(Refusal::PathNotRegularBlob),
        "a symlink at the artifact path is never followed"
    );
}

#[test]
fn a_tampered_runtime_file_refuses_on_its_checksum() {
    let release = release(|root| {
        fs::write(root.join("dist/launcher.js"), b"// swapped after staging\n").unwrap();
    });
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(outcome.err(), Some(Refusal::RuntimeClosure));
}

#[test]
fn a_manifest_from_another_tree_refuses_on_its_digest() {
    let mut release = release(|_root| {});
    release.manifest_digest = hb("amiss/scanner-release-manifest/v1", b"another tree");
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(outcome.err(), Some(Refusal::ManifestDigest));
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
    let lock = b"# Cargo.lock fixture\nversion = 4\n".to_vec();
    let binary_path = format!("dist/amiss-{}", platform.as_str());
    let mut artifacts = vec![StagedArtifact {
        platform,
        artifact_name: format!("amiss-{}", platform.as_str()),
        files: vec![StagedFile {
            path: binary_path.clone(),
            role: RuntimeRole::Executable,
            executable: true,
            bytes: &binary,
        }],
    }];
    let build = StagedBuild {
        engine_version: "0.1.0-experimental".to_owned(),
        owner: "hardmax71".to_owned(),
        repository: "amiss".to_owned(),
        object_format: "sha1",
        commit_oid: "a".repeat(40),
        locks: vec![("Cargo.lock".to_owned(), &lock)],
    };
    let (manifest_bytes, manifest_digest) = build_manifest(&build, &mut artifacts).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(
        root.join("action.yml"),
        action_metadata("Amiss", "Documentation assurance.", "dist/launcher.js"),
    )
    .unwrap();
    fs::write(root.join("release-manifest.json"), &manifest_bytes).unwrap();
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
        Some(Refusal::PlatformBinding),
        "the target comes from the executable header, not the manifest label"
    );
}
