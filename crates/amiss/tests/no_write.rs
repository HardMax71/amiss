use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

#[expect(clippy::expect_used, reason = "test fixture helper")]
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
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
}

/// One filesystem entry's complete identity: bytes for files, targets for
/// symlinks, presence for directories. Permissions are included whole, so a
/// permission flip counts as a change on whatever terms the platform keeps
/// them.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    File {
        permissions: fs::Permissions,
        digest: [u8; 32],
    },
    Symlink {
        target: PathBuf,
    },
    Directory {
        permissions: fs::Permissions,
    },
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("under root").to_path_buf();
            let metadata = fs::symlink_metadata(&path).expect("metadata");
            let kind = if metadata.file_type().is_symlink() {
                Entry::Symlink {
                    target: fs::read_link(&path).expect("read_link"),
                }
            } else if metadata.is_dir() {
                stack.push(path.clone());
                Entry::Directory {
                    permissions: metadata.permissions(),
                }
            } else {
                let bytes = fs::read(&path).expect("read file");
                Entry::File {
                    permissions: metadata.permissions(),
                    digest: Sha256::digest(&bytes).into(),
                }
            };
            out.insert(relative, kind);
        }
    }
    out
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn amiss(args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn fixture(with_governed: bool) -> (TempDir, String, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [gone](missing.md)\n",
    )
    .unwrap();
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy/v1","document_includes":[],"protected_inventory":["docs/guide.md"],"finding_dispositions":[]}"#,
    )
    .unwrap();
    if with_governed {
        fs::write(
            root.join("docs/governed.md"),
            "A [claim][amiss:c].\n\n[amiss:c]: ./x.md\n",
        )
        .unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("docs/extra.mdx"), "hello {1 + 1}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("docs/staged.md"), "# Staged\n\n[up](guide.md)\n").unwrap();
    git(root, &["add", "docs/staged.md"]);
    (dir, base, candidate)
}

/// Every command leaves repository status, index, refs, and bytes unchanged:
/// the complete tree under the repository root, `.git` included, is
/// byte-identical after each invocation, and nothing new appears.
#[test]
fn every_command_leaves_the_repository_byte_identical() {
    let (dir, base, candidate) = fixture(true);
    let root = dir.path();
    let repo = root.to_str().unwrap_or_default().to_owned();
    let before = snapshot(root);

    let runs: Vec<Vec<&str>> = vec![
        vec![
            "check",
            "--repo",
            &repo,
            "--object-format",
            "sha1",
            "--base",
            &base,
            "--candidate",
            &candidate,
            "--profile",
            "observe",
            "--format",
            "json",
        ],
        vec![
            "check",
            "--repo",
            &repo,
            "--object-format",
            "sha1",
            "--base",
            &base,
            "--candidate",
            &candidate,
            "--profile",
            "enforce",
        ],
        vec![
            "check",
            "--repo",
            &repo,
            "--object-format",
            "sha1",
            "--base",
            &base,
            "--index",
            "--profile",
            "observe",
            "--format",
            "json",
        ],
        vec![
            "check",
            "--repo",
            &repo,
            "--object-format",
            "sha1",
            "--base",
            &base,
            "--candidate",
            &candidate,
            "--profile",
            "observe",
            "--explain-scope",
        ],
        vec!["check", "--repo", &repo, "--not-a-flag"],
    ];
    for args in runs {
        let (code, _stdout) = amiss(&args);
        assert!((0..=2).contains(&code), "{args:?} exited {code}");
        let after = snapshot(root);
        assert_eq!(after, before, "the repository changed after {args:?}");
    }
}

/// Unix mode bits. Windows has no equivalent: the read-only attribute on a
/// directory does not stop a process creating files inside it, so a read-only
/// tree there would prove nothing.
#[cfg(unix)]
fn set_writable(root: &Path, writable: bool) {
    use std::os::unix::fs::PermissionsExt as _;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            let mode = if metadata.is_dir() {
                stack.push(path.clone());
                if writable { 0o755 } else { 0o555 }
            } else if writable {
                0o644
            } else {
                0o444
            };
            let _ignored = fs::set_permissions(&path, fs::Permissions::from_mode(mode));
        }
    }
    let root_mode = if writable { 0o755 } else { 0o555 };
    let _ignored = fs::set_permissions(root, fs::Permissions::from_mode(root_mode));
}

/// The stronger form: the scanner completes against a repository it cannot
/// write at all, in both commit-pair and staged-index modes.
#[cfg(unix)]
#[test]
fn a_read_only_repository_scans_completely() {
    let (dir, base, candidate) = fixture(false);
    let root = dir.path();
    let repo = root.to_str().unwrap_or_default().to_owned();
    set_writable(root, false);

    let (pair, pair_out) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--candidate",
        &candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    let (index, index_out) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &base,
        "--index",
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    set_writable(root, true);
    assert_eq!(
        pair, 0,
        "commit-pair mode needs no write access: {pair_out}"
    );
    assert_eq!(
        index, 0,
        "staged-index mode needs no write access: {index_out}"
    );
}
