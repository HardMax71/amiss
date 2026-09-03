use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use amiss_bootstrap::build::{RELEASE_MANIFEST_DIGEST_PATH, RELEASE_MANIFEST_PATH};
use amiss_wire::manifest::{canonical_release_manifest, parse_release_manifest};
use tempfile::TempDir;

#[test]
fn the_manifest_builder_publishes_its_digest_marker() {
    let tree = TempDir::new().unwrap();
    std::fs::write(tree.path().join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(tree.path().join("action.yml"), b"action").unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_BIN_EXE_amiss-bootstrap")),
        tree.path().join("engine"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
        .arg("--tree")
        .arg(tree.path())
        .args([
            "--version",
            "0.9.0",
            "--host",
            "git.example",
            "--owner",
            "platform",
            "--repository",
            "amiss",
            "--commit",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--action",
            "action.yml",
            "--lock",
            "Cargo.lock",
            "--artifact",
            "engine",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());

    let manifest =
        parse_release_manifest(&std::fs::read(tree.path().join(RELEASE_MANIFEST_PATH)).unwrap())
            .unwrap();
    let marker = std::fs::read_to_string(tree.path().join(RELEASE_MANIFEST_DIGEST_PATH)).unwrap();
    let digest = canonical_release_manifest(&manifest).unwrap().1;
    assert_eq!(marker, format!("{digest}\n"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), marker);
}

#[test]
fn a_lone_version_flag_asks_the_builder_not_the_release() {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("amiss-manifest {}\n", env!("CARGO_PKG_VERSION"))
    );

    let refused = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
        .args(["--version", "0.9.0"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(refused.stdout.is_empty());
}

/// Each staged list is required on its own: locks without artifacts is the
/// same usage refusal as artifacts without locks.
#[test]
fn a_half_empty_staging_list_is_refused() {
    let tree = TempDir::new().unwrap();
    std::fs::write(tree.path().join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(tree.path().join("action.yml"), b"action").unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_BIN_EXE_amiss-bootstrap")),
        tree.path().join("engine"),
    )
    .unwrap();
    let common = [
        "--version",
        "0.9.0",
        "--host",
        "git.example",
        "--owner",
        "platform",
        "--repository",
        "amiss",
        "--commit",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--action",
        "action.yml",
    ];
    for extra in [["--lock", "Cargo.lock"], ["--artifact", "engine"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
            .arg("--tree")
            .arg(tree.path())
            .args(common)
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{extra:?}: {output:?}");
    }
}

#[test]
fn every_singleton_flag_refuses_repetition() {
    let tree = TempDir::new().unwrap();
    std::fs::write(tree.path().join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(tree.path().join("action.yml"), b"action").unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_BIN_EXE_amiss-bootstrap")),
        tree.path().join("engine"),
    )
    .unwrap();
    let common = [
        "--version",
        "0.9.0",
        "--host",
        "git.example",
        "--owner",
        "platform",
        "--repository",
        "amiss",
        "--commit",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--action",
        "action.yml",
        "--lock",
        "Cargo.lock",
        "--artifact",
        "engine",
    ];
    let repeated = [
        ("--tree", tree.path().as_os_str()),
        ("--version", OsStr::new("0.9.0")),
        ("--host", OsStr::new("git.example")),
        ("--owner", OsStr::new("platform")),
        ("--repository", OsStr::new("amiss")),
        (
            "--commit",
            OsStr::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ),
        ("--action", OsStr::new("action.yml")),
    ];
    for (flag, value) in repeated {
        let output = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
            .arg("--tree")
            .arg(tree.path())
            .args(common)
            .arg(flag)
            .arg(value)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{flag}: {output:?}");
        assert!(output.stdout.is_empty(), "{flag}: {output:?}");
        assert_eq!(
            output.stderr, b"amiss-manifest: invalid-invocation\n",
            "{flag}: {output:?}"
        );
    }
}
