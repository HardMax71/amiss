#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

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

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn fixture() -> (TempDir, String, String) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n\n[home](../README)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [gone](missing.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    (dir, base, candidate)
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn amiss(args: &[&str]) -> (i32, Vec<u8>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(args)
        .output()
        .expect("run amiss");
    (
        output.status.code().unwrap_or(-1),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[expect(clippy::unwrap_used, reason = "differential test against the binary")]
fn payload(stdout: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(stdout).unwrap();
    envelope.get("payload").cloned().unwrap()
}

#[test]
fn a_clean_observe_run_passes_with_a_complete_report() {
    let (dir, base, candidate) = fixture();
    let repo = dir.path().to_str().unwrap_or_default().to_owned();
    let (code, stdout, stderr) = amiss(&[
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
    assert_eq!(
        (code, stderr.as_str()),
        (0, ""),
        "a passing observe run exits zero"
    );
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["result"]["exit_code"], 0);
    assert_eq!(payload["summary"]["references"]["missing"], 1);
}

#[test]
fn enforce_fails_on_a_missing_target() {
    let (dir, base, candidate) = fixture();
    let repo = dir.path().to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
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
        "--format",
        "json",
    ]);
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "fail");
    assert_eq!(payload["result"]["exit_code"], 1);
    assert_eq!(code, 1);
    let kinds: Vec<String> = payload["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|finding| finding["kind"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    assert!(kinds.iter().any(|kind| kind == "explicit-target-missing"));
}

#[test]
fn an_unreadable_repository_is_a_fatal_incomplete_envelope() {
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        "/nonexistent/amiss-test",
        "--object-format",
        "sha1",
        "--base",
        &"a".repeat(40),
        "--candidate",
        &"b".repeat(40),
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert_eq!(
        payload["errors"][0]["code"], "GIT_REPOSITORY_UNAVAILABLE",
        "the one error row names the defect"
    );
}

#[test]
fn index_mode_is_honestly_incomplete_for_now() {
    let (dir, base, _candidate) = fixture();
    let repo = dir.path().to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
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
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    assert_eq!(payload["errors"][0]["code"], "INTERNAL_ERROR");
}

#[test]
fn human_output_projects_the_same_result() {
    let (dir, base, candidate) = fixture();
    let repo = dir.path().to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
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
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(text.starts_with("amiss: pass ("), "got: {text}");
    assert!(text.contains("warn explicit-target-missing docs/guide.md"));
}
