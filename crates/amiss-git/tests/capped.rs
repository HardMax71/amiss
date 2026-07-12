#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_git::{Error, GitLimits, GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};
use tempfile::TempDir;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
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
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
}

fn body(version: usize) -> String {
    (0..200_usize)
        .map(|line| {
            if line == 100 {
                format!("changed line for version {version}\n")
            } else {
                format!("stable line {line} shared by both versions\n")
            }
        })
        .collect()
}

fn doc_cap(limit: u64) -> ValueCap {
    ValueCap {
        resource: ResourceName::DocumentBlobBytes,
        limit,
    }
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn read_capped(dir: &Path, hex: &str, limit: u64) -> Result<Vec<u8>, Error> {
    let repo = Repository::open(dir, ObjectFormat::Sha1).unwrap();
    let oid = Oid::new(ObjectFormat::Sha1, hex.to_owned()).unwrap();
    let mut res = GitResources::new(GitLimits::CONTRACT);
    repo.read_expected_capped(&mut res, &oid, ObjectKind::Blob, doc_cap(limit))
        .map(|object| object.body)
}

#[test]
fn a_loose_header_past_the_cap_reports_the_smaller_resource() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("doc.md"), body(1)).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    let blob = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    let declared = u64::try_from(body(1).len()).unwrap();

    assert_eq!(
        read_capped(dir.path(), &blob, 64),
        Err(Error::ResourceLimit {
            resource: ResourceName::DocumentBlobBytes,
            configured_limit: 64,
            observed_lower_bound: declared,
        }),
        "a loose header declares its size, so the crossing observes it exactly"
    );
    assert_eq!(
        read_capped(dir.path(), &blob, declared).unwrap(),
        body(1).into_bytes()
    );
}

#[test]
fn packed_and_deltified_objects_honor_the_cap() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("doc.md"), body(1)).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    let first = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    fs::write(dir.path().join("doc.md"), body(2)).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "two"]);
    let second = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    git(
        dir.path(),
        &["repack", "-adfq", "--window=10", "--depth=16"],
    );

    for (blob, version) in [(&first, 1_usize), (&second, 2_usize)] {
        let declared = u64::try_from(body(version).len()).unwrap_or(u64::MAX);
        assert_eq!(
            read_capped(dir.path(), blob, 64),
            Err(Error::ResourceLimit {
                resource: ResourceName::DocumentBlobBytes,
                configured_limit: 64,
                observed_lower_bound: declared,
            }),
            "a packed entry declares its final size, deltified or not"
        );
    }
    assert_eq!(
        read_capped(dir.path(), &first, 1 << 20).unwrap(),
        body(1).into_bytes()
    );
    assert_eq!(
        read_capped(dir.path(), &second, 1 << 20).unwrap(),
        body(2).into_bytes()
    );
}
