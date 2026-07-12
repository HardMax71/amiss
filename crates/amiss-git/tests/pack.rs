#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use amiss_git::{Error, GitLimits, GitResources, ObjectKind, Repository, parse_commit, parse_tree};
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

fn file_v(version: usize) -> String {
    (0..200_usize)
        .map(|line| {
            if line == 100 {
                format!("changed line for version {version}\n")
            } else {
                format!("stable line {line} with shared content\n")
            }
        })
        .collect()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn packed_repo(config: &[&str]) -> (TempDir, String, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("doc.md"), file_v(1)).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    let blob_v1 = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    fs::write(dir.path().join("doc.md"), file_v(2)).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "two"]);
    let blob_v2 = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    let mut repack: Vec<&str> = config.to_vec();
    repack.extend_from_slice(&["repack", "-adfq", "--window=10", "--depth=16"]);
    git(dir.path(), &repack);
    (dir, blob_v1, blob_v2)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn pack_paths(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir.join(".git/objects/pack"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    out.sort();
    out
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn deltified_oid(dir: &Path) -> Option<String> {
    let idx = pack_paths(dir)
        .into_iter()
        .find(|path| path.extension().is_some_and(|e| e == "idx"))?;
    let listing = git(dir, &["verify-pack", "-v", idx.to_str().unwrap()]);
    for line in listing.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 7 && matches!(fields.get(1), Some(&"blob")) {
            return fields.first().map(|oid| (*oid).to_owned());
        }
    }
    None
}

fn read_blob(repo: &Repository, hex: &str) -> Result<Vec<u8>, Error> {
    let oid = Oid::new(ObjectFormat::Sha1, hex.to_owned()).ok_or(Error::ObjectUnreadable)?;
    let mut res = GitResources::new(GitLimits::CONTRACT);
    repo.read_expected(&mut res, &oid, ObjectKind::Blob)
        .map(|object| object.body)
}

#[test]
fn reads_packed_objects_differentially() {
    let (dir, blob_v1, blob_v2) = packed_repo(&[]);
    assert!(
        deltified_oid(dir.path()).is_some(),
        "fixture must contain at least one delta"
    );
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(read_blob(&repo, &blob_v1).unwrap(), file_v(1).into_bytes());
    assert_eq!(read_blob(&repo, &blob_v2).unwrap(), file_v(2).into_bytes());

    let head = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_owned();
    let tree = git(dir.path(), &["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let head_oid = Oid::new(ObjectFormat::Sha1, head).unwrap();
    let commit_obj = repo
        .read_expected(&mut res, &head_oid, ObjectKind::Commit)
        .unwrap();
    let commit = parse_commit(ObjectFormat::Sha1, &commit_obj.body).unwrap();
    assert_eq!(commit.tree.as_str(), tree);
    assert_eq!(commit.parents.len(), 1);
    let tree_obj = repo
        .read_expected(&mut res, &commit.tree, ObjectKind::Tree)
        .unwrap();
    let entries = parse_tree(ObjectFormat::Sha1, &tree_obj.body).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries.first().unwrap().name, b"doc.md");

    let absent = Oid::new(ObjectFormat::Sha1, "c".repeat(40)).unwrap();
    assert_eq!(
        repo.read_object(&mut res, &absent).unwrap_err(),
        Error::ObjectMissing
    );
}

#[test]
fn reads_ref_delta_packs() {
    let (dir, blob_v1, blob_v2) = packed_repo(&["-c", "pack.useDeltaBaseOffset=false"]);
    assert!(
        deltified_oid(dir.path()).is_some(),
        "fixture must contain a delta"
    );
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(read_blob(&repo, &blob_v1).unwrap(), file_v(1).into_bytes());
    assert_eq!(read_blob(&repo, &blob_v2).unwrap(), file_v(2).into_bytes());
}

#[test]
fn reads_index_v1_packs() {
    let (dir, blob_v1, _) = packed_repo(&["-c", "pack.indexVersion=1"]);
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(read_blob(&repo, &blob_v1).unwrap(), file_v(1).into_bytes());
}

#[test]
fn reads_sha256_packs() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "--object-format=sha256"]);
    fs::write(dir.path().join("doc.md"), b"sha256 body\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "one"]);
    let blob = git(dir.path(), &["rev-parse", "HEAD:doc.md"])
        .trim()
        .to_owned();
    git(dir.path(), &["repack", "-adq"]);
    let repo = Repository::open(dir.path(), ObjectFormat::Sha256).unwrap();
    let oid = Oid::new(ObjectFormat::Sha256, blob).unwrap();
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let object = repo
        .read_expected(&mut res, &oid, ObjectKind::Blob)
        .unwrap();
    assert_eq!(object.body, b"sha256 body\n");
}

#[test]
fn rejects_orphans_and_corruption() {
    let (dir, blob_v1, _) = packed_repo(&[]);
    let idx = pack_paths(dir.path())
        .into_iter()
        .find(|path| path.extension().is_some_and(|e| e == "idx"))
        .unwrap();
    let pack = idx.with_extension("pack");

    let saved = fs::read(&pack).unwrap();
    fs::remove_file(&pack).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(
        read_blob(&repo, &blob_v1).unwrap_err(),
        Error::ObjectUnreadable,
        "orphan idx without its pack is fatal"
    );
    fs::write(&pack, &saved).unwrap();

    let mut corrupt = fs::read(&idx).unwrap();
    let middle = corrupt.len() / 2;
    if let Some(byte) = corrupt.get_mut(middle) {
        *byte = byte.wrapping_add(1);
    }
    fs::remove_file(&idx).unwrap();
    fs::write(&idx, &corrupt).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(
        read_blob(&repo, &blob_v1).unwrap_err(),
        Error::ObjectUnreadable,
        "index checksum mismatch is fatal"
    );
}

#[test]
fn enforces_pack_resource_caps() {
    let (dir, blob_v1, _) = packed_repo(&[]);

    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let oid = Oid::new(ObjectFormat::Sha1, blob_v1.clone()).unwrap();
    let mut res = GitResources::new(GitLimits {
        pack_directory_entries: 1,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit { resource, .. }) = repo.read_object(&mut res, &oid) else {
        panic!("expected the directory-entry cap");
    };
    assert_eq!(resource, ResourceName::GitPackDirectoryEntries);

    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut res = GitResources::new(GitLimits {
        pack_files: 0,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit { resource, .. }) = repo.read_object(&mut res, &oid) else {
        panic!("expected the pack-files cap");
    };
    assert_eq!(resource, ResourceName::GitPackFiles);

    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut res = GitResources::new(GitLimits {
        pack_index_bytes: 8,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit { resource, .. }) = repo.read_object(&mut res, &oid) else {
        panic!("expected the index-bytes cap");
    };
    assert_eq!(resource, ResourceName::GitPackIndexBytes);

    let deltified = deltified_oid(dir.path()).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let delta_oid = Oid::new(ObjectFormat::Sha1, deltified).unwrap();
    let mut res = GitResources::new(GitLimits {
        delta_depth: 1,
        ..GitLimits::CONTRACT
    });
    let Err(Error::ResourceLimit {
        resource,
        configured_limit,
        observed_lower_bound,
    }) = repo.read_object(&mut res, &delta_oid)
    else {
        panic!("expected the delta-depth cap");
    };
    assert_eq!(resource, ResourceName::GitDeltaDepth);
    assert_eq!(configured_limit, 1);
    assert_eq!(observed_lower_bound, 2);
}

#[test]
fn duplicate_objects_across_packs_resolve() {
    let (dir, blob_v1, _) = packed_repo(&[]);
    fs::write(dir.path().join("extra.md"), b"more\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "three"]);
    git(dir.path(), &["repack", "-aq"]);
    assert!(pack_paths(dir.path()).len() >= 4, "two pack pairs expected");
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    assert_eq!(read_blob(&repo, &blob_v1).unwrap(), file_v(1).into_bytes());
}
