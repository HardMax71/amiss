use std::path::PathBuf;
use std::process::Command;

use amiss_bootstrap::build::{RELEASE_MANIFEST_DIGEST_PATH, RELEASE_MANIFEST_PATH};
use amiss_wire::manifest::ReleaseManifest;
use tempfile::TempDir;

#[test]
fn the_manifest_builder_publishes_its_digest_marker() {
    let tree = TempDir::new().unwrap();
    std::fs::write(tree.path().join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(tree.path().join("action.yml"), b"action").unwrap();
    std::fs::write(tree.path().join("launcher.js"), b"launcher").unwrap();
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
            "--launcher",
            "launcher.js",
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
        ReleaseManifest::parse(&std::fs::read(tree.path().join(RELEASE_MANIFEST_PATH)).unwrap())
            .unwrap();
    let marker = std::fs::read_to_string(tree.path().join(RELEASE_MANIFEST_DIGEST_PATH)).unwrap();
    assert_eq!(marker, format!("{}\n", manifest.digest));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), marker);
}

/// `--version V` stamps the release under construction; a lone `--version`
/// asks the builder about itself. The two must not be confused.
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
