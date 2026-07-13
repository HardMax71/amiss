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
fn index_mode_scans_the_staged_snapshot() {
    let (dir, _base, candidate) = fixture();
    let root = dir.path();
    fs::write(root.join("docs/staged.md"), "# Staged\n\n[up](guide.md)\n").unwrap_or_default();
    git(root, &["add", "docs/staged.md"]);
    fs::write(
        root.join("docs/staged.md"),
        "worktree drift with [broken](nowhere.md)\n",
    )
    .unwrap_or_default();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &candidate,
        "--index",
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["evaluation"]["mode"], "index");
    assert_eq!(payload["evaluation"]["materialization"], "index");
    assert_eq!(payload["evaluation"]["candidate"]["kind"], "index");
    assert!(
        payload["evaluation"]["candidate"]["entry_count"]
            .as_u64()
            .unwrap_or(0)
            >= 3
    );
    let documents: Vec<&str> = payload["documents"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["path"].as_str()).collect())
        .unwrap_or_default();
    assert!(documents.contains(&"docs/staged.md"));
    assert_eq!(
        payload["summary"]["references"]["missing"].as_u64(),
        Some(1),
        "only the committed missing.md link is missing; the worktree drift is never read"
    );
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
    assert!(text.starts_with("amiss: pass (findings "), "got: {text}");
    assert!(
        text.contains("warn explicit-target-missing introduced \"docs/guide.md\" x1"),
        "the path is an inert quoted atom: {text}"
    );
    assert!(
        text.contains("references: extracted "),
        "totals close the projection"
    );
    assert!(!text.contains('\r'), "LF-only stdout");
}

#[test]
fn repository_policy_includes_raises_and_weakening() {
    let (dir, _base, _candidate) = fixture();
    let root = dir.path();

    let strong_policy = r#"{"schema":"amiss/scanner-policy/v1","document_includes":[{"kind":"tree","path":"specs"}],"protected_inventory":["docs/guide.md"],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#;
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::create_dir_all(root.join("specs")).unwrap_or_default();
    fs::write(root.join(".amiss/scanner-policy.json"), strong_policy).unwrap_or_default();
    fs::write(root.join("specs/design.rst"), "included but unsupported\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "policy"]);
    let with_policy = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy/v1","document_includes":[],"protected_inventory":["docs/guide.md"],"finding_dispositions":[]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "weakened"]);
    let weakened = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &with_policy,
        "--candidate",
        &weakened,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "weakening is an unsuppressible fail");
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "fail");
    assert!(
        payload["controls"]["base_repository_policy_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );
    let rows: Vec<(String, String)> = payload["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|finding| {
                    Some((
                        finding["kind"].as_str()?.to_owned(),
                        finding["key_input"]["scope"]["rule_id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(rows.contains(&(
        "policy-weakened".to_owned(),
        "policy/include-tree-removed".to_owned()
    )));
    assert!(rows.contains(&(
        "policy-weakened".to_owned(),
        "policy/disposition/explicit-target-missing".to_owned()
    )));
    let documents: Vec<(&str, &str)> = payload["documents"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| Some((row["path"].as_str()?, row["classification"].as_str()?)))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        documents.contains(&("specs/design.rst", "policy-included")),
        "the include is discovered without installing a parser: {documents:?}"
    );
}

#[test]
fn a_raised_disposition_fails_a_passing_observe_run() {
    let (dir, _base, candidate) = fixture();
    let root = dir.path();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy/v1","document_includes":[],"protected_inventory":[],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#,
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "raise"]);
    let raised = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &candidate,
        "--candidate",
        &raised,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "the raise turns the missing target into fail");
    let payload = payload(&stdout);
    let missing = payload["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["kind"] == "explicit-target-missing")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(missing["effective_disposition"], "fail");
    assert_eq!(missing["configured_disposition"], "warn");
    assert_eq!(
        missing["policy_trace"][1]["rule_id"], "repository/explicit-target-missing",
        "the repository step follows the built-in step"
    );
}

#[test]
fn an_invalid_policy_is_fatal_with_unavailable_controls() {
    let (dir, _base, _candidate) = fixture();
    let root = dir.path();
    fs::create_dir_all(root.join(".amiss")).unwrap_or_default();
    fs::write(root.join(".amiss/scanner-policy.json"), "{not json").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "broken"]);
    let broken = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("touch.md"), "later\n").unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "later"]);
    let later = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &broken,
        "--candidate",
        &later,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let payload = payload(&stdout);
    assert_eq!(payload["controls"]["status"], "unavailable");
    assert_eq!(
        payload["controls"]["reasons"][0],
        "invalid-repository-policy"
    );
    let codes: Vec<&str> = payload["errors"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["code"].as_str()).collect())
        .unwrap_or_default();
    assert!(codes.contains(&"CONFIGURATION_INVALID"));
    assert!(
        payload["errors"][0]["path"] == ".amiss/scanner-policy.json"
            || payload["errors"][1]["path"] == ".amiss/scanner-policy.json"
    );
}

#[test]
fn reserved_directives_are_boundary_incomplete_with_full_details() {
    let (dir, _base, candidate) = fixture();
    let root = dir.path();
    fs::write(
        root.join("docs/governed.md"),
        "A claim [here][amiss:claim v1] and [fine](guide.md).\n\n\
         [amiss:claim v1]: ./subject.md \"claim\"\n\
         [amiss:claim v1]: ./subject.md \"claim\"\n",
    )
    .unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "governed"]);
    let governed = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &candidate,
        "--candidate",
        &governed,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2, "governed syntax exits two under either profile");
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert_eq!(payload["result"]["complete"], false);
    assert!(
        !payload["documents"]
            .as_array()
            .unwrap_or(&Vec::new())
            .is_empty(),
        "boundary-incomplete keeps complete detail arrays"
    );
    assert_eq!(payload["errors"][0]["code"], "UNSUPPORTED_CAPABILITY");
    assert_eq!(payload["errors"][0]["path"], "docs/governed.md");
    assert_eq!(payload["errors"][0]["phase"], "policy");

    let finding = payload["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["kind"] == "unsupported-capability")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        finding["key_input"]["scope"]["rule_id"],
        "unsupported/governed-claim"
    );
    assert_eq!(
        finding["key_input"]["scope"]["control_path"],
        "docs/governed.md"
    );
    assert_eq!(
        finding["aggregation"]["member_count"], 2,
        "two nodes, one duplicated source"
    );
    assert_eq!(finding["effective_disposition"], "fail");
    let sources = &finding["candidate_fact"]["evidence"]["candidate_control_state"]["sources"];
    assert_eq!(
        sources.as_array().map(Vec::len),
        Some(1),
        "equal digests group"
    );
    assert_eq!(sources[0]["multiplicity"], 2);

    let suppressed: Vec<&str> = payload["observations"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["candidate"]["document"].as_str())
                .filter(|document| *document == "docs/governed.md")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        suppressed.len(),
        1,
        "only the ordinary link is an observation; the governed consumer is suppressed"
    );
}

#[test]
fn human_details_truncate_at_two_hundred() {
    let (dir, _base, candidate) = fixture();
    let root = dir.path();
    let mut links = Vec::new();
    for index in 0..201 {
        links.push(format!("[l{index}](absent-{index}.md)"));
    }
    let body = format!("# Many\n\n{}\n", links.join("\n\n"));
    fs::write(root.join("docs/many.md"), body).unwrap_or_default();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "many"]);
    let many = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &candidate,
        "--candidate",
        &many,
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    let detail_lines = text
        .lines()
        .filter(|line| line.starts_with("warn explicit-target-missing"))
        .count();
    assert_eq!(
        detail_lines, 200,
        "the first two hundred findings in key order"
    );
    assert!(text.contains("details truncated: "), "{text}");

    let (_code, stdout, _stderr) = amiss(&[
        "check",
        "--repo",
        &repo,
        "--object-format",
        "sha1",
        "--base",
        &candidate,
        "--candidate",
        &many,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    let payload = payload(&stdout);
    assert_eq!(
        payload["summary"]["human_details_truncated"].as_u64(),
        payload["result"]["finding_count"]
            .as_u64()
            .map(|count| count.saturating_sub(200)),
        "the payload records the truncation regardless of format"
    );
}

#[test]
fn explain_scope_adds_the_deterministic_block() {
    let (dir, base, candidate) = fixture();
    let repo = dir.path().to_str().unwrap_or_default().to_owned();
    let run = |extra: &[&str]| {
        let mut args = vec![
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
        ];
        args.extend_from_slice(extra);
        amiss(&args)
    };
    let (_c, plain, _e) = run(&[]);
    let (_c, explained, _e) = run(&["--explain-scope"]);
    let plain = String::from_utf8_lossy(&plain);
    let explained = String::from_utf8_lossy(&explained);
    assert!(!plain.contains("scope:"));
    assert!(explained.contains("scope: built-in documents"));
    assert!(explained.contains("scope: this run discovered"));
}
