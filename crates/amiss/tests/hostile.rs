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

fn amiss(args: &[&str]) -> (i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (output.status.code().unwrap_or(-1), output.stdout)
}

fn payload(stdout: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    envelope["payload"].clone()
}

/// The repository under evaluation is the attacker. It writes the policy the
/// scanner reads, so the one thing that policy may never do is widen what the
/// scanner is allowed to do. A field naming a command or a plugin is not a
/// feature request the scanner declines politely: it is an unknown field, the
/// configuration is invalid, the run is incomplete, and there is no report to
/// mistake for a pass. The sentinel proves the obvious thing anyway, because the
/// obvious thing is the whole product.
#[test]
fn a_policy_that_names_a_command_or_a_plugin_is_refused_and_nothing_runs() {
    let sentinel = std::env::temp_dir().join("amiss-policy-execution-sentinel");
    let _absent = fs::remove_file(&sentinel);

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n\n[self](guide.md)\n").unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        format!(
            r#"{{
  "schema": "amiss/scanner-policy/v1",
  "document_includes": [],
  "protected_inventory": [],
  "finding_dispositions": [],
  "command": "touch {}",
  "plugin": "./evil.so"
}}"#,
            sentinel.display()
        ),
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[self](guide.md)\n\nmore\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);

    let repo = root.to_str().unwrap().to_owned();
    let (code, stdout) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        git(root, &["rev-parse", "HEAD~1"]).trim(),
        "--candidate",
        git(root, &["rev-parse", "HEAD"]).trim(),
        "--profile",
        "observe",
        "--format",
        "json",
    ]);

    assert_eq!(
        code, 2,
        "a policy it cannot read is not a policy it ignores"
    );
    let payload = payload(&stdout);
    let mut codes: Vec<&str> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| error["code"].as_str().unwrap())
        .collect();
    codes.sort_unstable();
    assert_eq!(codes, vec!["CONFIGURATION_INVALID", "UNKNOWN_FIELD"]);
    assert_eq!(payload["result"]["complete"], false);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert!(
        !sentinel.exists(),
        "the policy's command ran and wrote {}",
        sentinel.display()
    );
}

/// In index mode the candidate is the staged index, and the staged index is the
/// whole of it. A file sitting in the worktree that nobody staged is not part of
/// the tree being evaluated, so a reference to it does not resolve, and the
/// finding stands. Getting this wrong needs only one `fs::metadata` call
/// somewhere in resolution, and it would be invisible: every reference would
/// still resolve, the report would still pass, and the tool would be answering a
/// question about the developer's disk instead of the commit under review.
#[test]
fn an_untracked_file_cannot_satisfy_an_index_mode_reference() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[later](arriving.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join("docs/arriving.md"),
        "# Arriving\n\nbut never staged\n",
    )
    .unwrap();
    assert!(
        root.join("docs/arriving.md").exists(),
        "the target is on disk, and only on disk"
    );

    let repo = root.to_str().unwrap().to_owned();
    let (code, stdout) = amiss(&[
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

    assert_eq!(code, 0, "observe reports rather than blocks");
    let payload = payload(&stdout);
    assert_eq!(
        payload["summary"]["references"]["missing"], 1,
        "the reference is still missing, because the file it names is not staged"
    );
    let documents: Vec<&str> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert!(
        !documents.contains(&"docs/arriving.md"),
        "an untracked file is not a document either: {documents:?}"
    );
}
