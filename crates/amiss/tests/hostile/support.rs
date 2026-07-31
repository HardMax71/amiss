#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

pub(crate) fn amiss(args: &[&str]) -> (i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (output.status.code().unwrap_or(-1), output.stdout)
}

pub(crate) fn payload(stdout: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    envelope["payload"].clone()
}

/// Runs a repository whose tree or index carries one entry named `name`,
/// alongside two documents the scanner can read. The entry is written straight
/// into the store or index bytes, past any git port's opinion of the name.
pub(crate) fn hidden_entry(name: &[u8], index_mode: bool) -> (i32, serde_json::Value) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# R\n\n[g](docs/guide.md)\n").unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let guide = git(root, &["rev-parse", "HEAD:docs/guide.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# Hidden\n").unwrap();

    let candidate = if index_mode {
        amiss_fixtures::index_file(
            root,
            &[
                (b"README.md".as_slice(), readme.as_str()),
                (b"docs/guide.md".as_slice(), guide.as_str()),
                (name, blob.as_str()),
            ],
        )
        .unwrap();
        String::new()
    } else {
        let docs_entries: Vec<(&str, &[u8], &str)> = match name.strip_prefix(b"docs/") {
            Some(inner) => vec![
                ("100644", b"guide.md".as_slice(), guide.as_str()),
                ("100644", inner, blob.as_str()),
            ],
            None => vec![("100644", b"guide.md".as_slice(), guide.as_str())],
        };
        let docs = amiss_fixtures::tree_object(root, &docs_entries).unwrap();
        let mut root_entries: Vec<(&str, &[u8], &str)> = vec![
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("40000", b"docs".as_slice(), docs.as_str()),
        ];
        if !name.contains(&b'/') {
            root_entries.push(("100644", name, blob.as_str()));
        }
        let tree = amiss_fixtures::tree_object(root, &root_entries).unwrap();
        amiss_fixtures::commit_object(root, &tree, &[&base], "candidate").unwrap()
    };
    let repo = amiss_fixtures::path_arg(root);
    let (code, stdout) = if index_mode {
        amiss(&[
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
        ])
    } else {
        amiss(&[
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
        ])
    };
    (code, payload(&stdout))
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn byte_named_index(content: &[u8]) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", content).unwrap();
    amiss_fixtures::index_file(
        root,
        &[
            (b"README.md".as_slice(), readme.as_str()),
            (b"bad-\xff-doc.md".as_slice(), blob.as_str()),
        ],
    )
    .unwrap();
    (dir, base)
}

pub(crate) const BYTE_NAME_HEX: &str = "6261642dff2d646f632e6d64";
