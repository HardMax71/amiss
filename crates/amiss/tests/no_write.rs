use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest as _, Sha256};

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
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
fn fixture(with_governed: bool) -> amiss_fixtures::CommitPair {
    let mut base = vec![
        ("README", "See [the guide](docs/guide.md).\n"),
        (
            "docs/guide.md",
            "# Guide\n\n[home](../README) and [gone](missing.md)\n",
        ),
        (
            ".amiss/scanner-policy.json",
            r#"{"schema":"amiss/scanner-policy/v1","document_includes":[],"protected_inventory":["docs/guide.md"],"finding_dispositions":[]}"#,
        ),
    ];
    if with_governed {
        base.push((
            "docs/governed.md",
            "A [claim][amiss:c].\n\n[amiss:c]: ./x.md\n",
        ));
    }
    let fx = amiss_fixtures::commit_pair(&base, &[("docs/extra.mdx", "hello {1 + 1}\n")]).unwrap();
    fs::write(
        fx.root().join("docs/staged.md"),
        "# Staged\n\n[up](guide.md)\n",
    )
    .unwrap();
    git(fx.root(), &["add", "docs/staged.md"]);
    fx
}

/// Every command leaves repository status, index, refs, and bytes unchanged:
/// the complete tree under the repository root, `.git` included, is
/// byte-identical after each invocation, and nothing new appears.
#[test]
fn every_command_leaves_the_repository_byte_identical() {
    let fx = fixture(true);
    let root = fx.root();
    let before = snapshot(root);

    let runs: Vec<Vec<&str>> = vec![
        vec![
            "check",
            "--repo",
            &fx.repo,
            "--object-format",
            "sha1",
            "--base",
            &fx.base,
            "--candidate",
            &fx.candidate,
            "--profile",
            "observe",
            "--format",
            "json",
        ],
        vec![
            "check",
            "--repo",
            &fx.repo,
            "--object-format",
            "sha1",
            "--base",
            &fx.base,
            "--candidate",
            &fx.candidate,
            "--profile",
            "enforce",
        ],
        vec![
            "check",
            "--repo",
            &fx.repo,
            "--object-format",
            "sha1",
            "--base",
            &fx.base,
            "--index",
            "--profile",
            "observe",
            "--format",
            "json",
        ],
        vec![
            "check",
            "--repo",
            &fx.repo,
            "--object-format",
            "sha1",
            "--base",
            &fx.base,
            "--candidate",
            &fx.candidate,
            "--profile",
            "observe",
            "--explain-scope",
        ],
        vec!["check", "--repo", &fx.repo, "--not-a-flag"],
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
    let fx = fixture(false);
    let root = fx.root();
    set_writable(root, false);

    let (pair, pair_out) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    let (index, index_out) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
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
