#![expect(
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use amiss_bootstrap::BOOTSTRAP_EXECUTABLE_BYTES;
use amiss_bootstrap::validate;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::model::{ObjectFormat, RepositoryIdentity};
use tempfile::TempDir;

mod support;

use support::release::{Release, release};

const BINARY: &str = env!("CARGO_BIN_EXE_amiss-constraint");

fn command(release: &Release, commit: &str, bootstrap: &Path, output: &Path) -> Command {
    let mut command = Command::new(BINARY);
    command
        .arg("--action-repository")
        .arg(release.dir.path())
        .arg("--action-identity")
        .arg("git.example.internal/platform/security/amiss")
        .arg("--action-commit")
        .arg(commit)
        .arg("--bootstrap")
        .arg(bootstrap)
        .arg("--required-status-name")
        .arg("amiss / assure")
        .arg("--output")
        .arg(output);
    command
}

fn arguments(release: &Release, output: &Path) -> Vec<OsString> {
    command(release, &release.commit, Path::new(BINARY), output)
        .get_args()
        .map(OsString::from)
        .collect()
}

fn invoke(release: &Release, output: &Path) -> Output {
    command(release, &release.commit, Path::new(BINARY), output)
        .output()
        .unwrap()
}

#[test]
fn writes_one_canonical_constraint_without_clobbering() {
    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    let first = destination.path().join("execution constraint.json");
    let output = invoke(&release, &first);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());

    let bytes = std::fs::read(&first).unwrap();
    let descriptor = ExecutionConstraintDescriptor::parse(&bytes).unwrap();
    assert_eq!(descriptor.canonical_bytes().unwrap(), bytes);
    assert_eq!(
        descriptor.action_repository,
        RepositoryIdentity::new(
            "git.example.internal".to_owned(),
            "platform/security".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap()
    );
    assert_eq!(descriptor.action_commit_oid.as_str(), release.commit);
    assert_eq!(descriptor.action_tree_oid.as_str(), release.tree);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}\n", descriptor.digest)
    );

    let bootstrap = std::fs::read(BINARY).unwrap();
    let repository = Repository::open(release.dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    validate(&repository, &mut resources, &descriptor, &bootstrap).unwrap();

    let second = destination.path().join("second.json");
    assert!(invoke(&release, &second).status.success());
    assert_eq!(std::fs::read(second).unwrap(), bytes);

    std::fs::write(&first, b"operator-owned").unwrap();
    let output = invoke(&release, &first);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(first).unwrap(), b"operator-owned");
}

#[cfg(unix)]
#[test]
fn does_not_clobber_a_linked_output() {
    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    let target = destination.path().join("target.json");
    let linked = destination.path().join("linked.json");
    std::fs::write(&target, b"operator-owned").unwrap();
    std::os::unix::fs::symlink(&target, &linked).unwrap();
    let output = invoke(&release, &linked);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&linked).unwrap(), b"operator-owned");
    assert_eq!(std::fs::read(target).unwrap(), b"operator-owned");
}

#[test]
fn rejects_bad_grammar_and_leaves_no_semantic_failure_output() {
    let empty = Command::new(BINARY).output().unwrap();
    assert_eq!(empty.status.code(), Some(2));
    assert!(empty.stdout.is_empty());
    let diagnostic = String::from_utf8_lossy(&empty.stderr);
    assert!(diagnostic.contains("invalid-invocation"));
    assert!(diagnostic.contains("usage: amiss-constraint"));

    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    let invalid = destination.path().join("invalid.json");
    let base = arguments(&release, &invalid);
    let mut cases = vec![
        vec![OsString::from("--help")],
        {
            let mut arguments = base.clone();
            arguments.extend([OsString::from("--unknown"), OsString::from("value")]);
            arguments
        },
        {
            let mut arguments = base.clone();
            let _value = arguments.pop();
            arguments
        },
        {
            let mut arguments = base.clone();
            arguments.extend([OsString::from("--output"), invalid.clone().into_os_string()]);
            arguments
        },
    ];
    for (flag, value) in [
        ("--action-identity", OsString::from("git.example/Bad/amiss")),
        ("--action-commit", OsString::from("abc")),
        ("--action-commit", OsString::from("a".repeat(64))),
        ("--required-status-name", OsString::from(" bad")),
        ("--action-repository", OsString::from("relative")),
        ("--bootstrap", OsString::from("relative")),
        ("--output", OsString::from("relative")),
    ] {
        let mut arguments = base.clone();
        replace_value(&mut arguments, flag, value);
        cases.push(arguments);
    }
    for arguments in cases {
        let output = Command::new(BINARY).args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(!invalid.exists());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage: amiss-constraint"),
            "{output:?}"
        );
    }

    let missing = destination.path().join("missing.json");
    let output = command(&release, &"f".repeat(40), Path::new(BINARY), &missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!missing.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("action-tree-mismatch"));

    let reordered = destination.path().join("reordered.json");
    let arguments = arguments(&release, &reordered);
    let reversed: Vec<OsString> = arguments.chunks_exact(2).rev().flatten().cloned().collect();
    let output = Command::new(BINARY).args(reversed).output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(reordered.is_file());
}

fn replace_value(arguments: &mut [OsString], flag: &str, value: OsString) {
    let position = arguments
        .iter()
        .position(|argument| argument == flag)
        .unwrap()
        .saturating_add(1);
    *arguments.get_mut(position).unwrap() = value;
}

#[test]
fn keeps_trust_files_outside_the_action_tree_after_resolution() {
    let release = release(|root| {
        std::fs::create_dir(root.join("operator")).unwrap();
        std::fs::copy(BINARY, root.join("bootstrap")).unwrap();
    });
    let destination = TempDir::new().unwrap();
    let external_output = destination.path().join("external.json");
    let nested_bootstrap = release.dir.path().join("bootstrap");
    let output = command(
        &release,
        &release.commit,
        &nested_bootstrap,
        &external_output,
    )
    .output()
    .unwrap();
    assert_refused(&output, &external_output, "trust-path-overlap");

    let nested_output = release.dir.path().join("constraint.json");
    assert_refused(
        &invoke(&release, &nested_output),
        &nested_output,
        "trust-path-overlap",
    );

    let alias = destination.path().join("alias");
    amiss_fixtures::directory_link(release.dir.path(), &alias).unwrap();
    let aliased_bootstrap = alias.join("bootstrap");
    let aliased_output = destination.path().join("aliased-output.json");
    let output = command(
        &release,
        &release.commit,
        &aliased_bootstrap,
        &aliased_output,
    )
    .output()
    .unwrap();
    assert_refused(&output, &aliased_output, "trust-path-overlap");

    let aliased_parent_output = alias.join("operator").join("constraint.json");
    assert_refused(
        &invoke(&release, &aliased_parent_output),
        &aliased_parent_output,
        "trust-path-overlap",
    );
}

#[test]
fn rejects_non_regular_and_oversized_bootstraps() {
    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    let output_path = destination.path().join("constraint.json");
    let directory = destination.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    let output = command(&release, &release.commit, &directory, &output_path)
        .output()
        .unwrap();
    assert_refused(&output, &output_path, "bootstrap-unreadable");

    let oversized = destination.path().join("oversized");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(BOOTSTRAP_EXECUTABLE_BYTES.saturating_add(1))
        .unwrap();
    let output = command(&release, &release.commit, &oversized, &output_path)
        .output()
        .unwrap();
    assert_refused(&output, &output_path, "bootstrap-unreadable");
}

#[cfg(unix)]
#[test]
fn rejects_a_linked_bootstrap() {
    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    std::fs::write(destination.path().join("bootstrap-target"), b"replacement").unwrap();
    let link = destination.path().join("bootstrap-link");
    std::os::unix::fs::symlink("bootstrap-target", &link).unwrap();
    let output_path = destination.path().join("constraint.json");
    let output = command(&release, &release.commit, &link, &output_path)
        .output()
        .unwrap();
    assert_refused(&output, &output_path, "bootstrap-unreadable");
}

#[test]
fn a_closed_digest_consumer_does_not_change_the_published_result() {
    let release = release(|_root| {});
    let destination = TempDir::new().unwrap();
    let output_path = destination.path().join("constraint.json");
    let mut child = command(&release, &release.commit, Path::new(BINARY), &output_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output_path.is_file());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

fn assert_refused(output: &Output, path: &Path, reason: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(!path.exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains(reason));
}
