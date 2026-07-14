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

/// `${{ github.repository }}` is `Owner/Name`, capitals and all, and the engine
/// requires the canonical lowercase identity. It will not fold the value itself:
/// the CLI's repository is a claim it cannot authenticate, the report has no
/// field to record what was actually typed, and the wrapper that folds an
/// authenticated identity is the layer allowed to do that. What the engine owes
/// instead is a refusal that can be acted on, because there is no `--help` and a
/// bare error code is not documentation.
#[test]
fn a_noncanonical_repository_owner_is_refused_in_terms_the_caller_can_act_on() {
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
        "--repository",
        "github.com/HardMax71/amiss",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--profile",
        "observe",
    ]);
    assert_eq!(code, 2, "an event it cannot trust is never a result");
    assert!(stdout.is_empty(), "a refusal is not a report");
    assert!(stderr.contains("INVALID_EVENT"), "{stderr}");
    assert!(
        stderr.contains("lowercase"),
        "the refusal names the contract it enforced: {stderr}"
    );
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
    assert_eq!(
        missing["configured_disposition"], "fail",
        "configured is the value after the repository and floor steps"
    );
    assert_eq!(missing["policy_trace"][0]["before"], "record");
    assert_eq!(missing["policy_trace"][0]["after"], "warn");
    assert_eq!(
        missing["policy_trace"][1]["rule_id"], "repository/explicit-target-missing",
        "the repository step follows the built-in step"
    );
    assert_eq!(missing["policy_trace"][1]["before"], "warn");
    assert_eq!(missing["policy_trace"][1]["after"], "fail");
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

/// Four suites validate a report against the frozen schema, and every one of them
/// builds that report in process. Nothing had ever read the bytes the binary
/// prints, which is the only artifact a caller ever sees. Those bytes are exactly
/// `JCS(envelope)` and one LF: canonical JSON puts the whole envelope on a single
/// line, so the trailing newline is the only newline in the stream. The serializer
/// is shared, so this passes the day it is written. What it buys is that it cannot
/// quietly stop passing.
#[test]
fn the_bytes_the_binary_prints_are_a_schema_clean_report() {
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
        "a complete accepted projection leaves stderr empty"
    );
    let (last, rest) = stdout.split_last().expect("the report is not empty");
    assert_eq!(*last, b'\n', "the report ends in an LF");
    assert!(
        !rest.contains(&b'\n'),
        "the canonical envelope is one line, so its LF is the only one"
    );

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report-v1.schema.json"),
    )
    .unwrap()
    .replace("assure/", "amiss/");
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "the bytes the binary printed are a schema-clean report"
    );
}

/// The README promises that a document Amiss cannot decode fails the run instead of
/// vanishing from it, and that promise is the whole product: a checker that quietly
/// skips what it cannot read reports a success it never earned. Every piece of this
/// was tested at its own layer and the pieces were never joined, so nothing drove a
/// repository holding an undecodable document through the command and looked at what
/// came back. What comes back is nothing: the document is named in a retained error,
/// the run is incomplete, and the exit is 2.
#[test]
fn a_document_it_cannot_decode_fails_the_run_instead_of_vanishing_from_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "# R\n\n[g](docs/guide.md)\n").unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join("docs/bad.md"),
        b"# Bad \xff\xfe\n\n[x](../README.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
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
        "--format",
        "json",
    ]);

    assert_eq!(
        code, 2,
        "an unreadable document is not a passing observe run"
    );
    let payload = payload(&stdout);
    assert_eq!(payload["result"]["complete"], false);
    assert_eq!(payload["result"]["status"], "incomplete");
    let errors = payload["errors"].as_array().unwrap();
    let invalid = errors
        .iter()
        .find(|error| error["code"] == "DOCUMENT_INVALID")
        .expect("the document it could not decode is disclosed");
    assert_eq!(
        invalid["path"], "docs/bad.md",
        "the error names the document, not just the failure"
    );
}

/// Reformatting a file a document points at changes the target's bytes and nothing
/// else. Amiss has no opinion about whether the prose is now wrong, and it must not
/// grow one: the raw digest moved, the block that references it did not, and that is
/// the entire claim. So the impact is advisory. It stays a warning under enforce,
/// where a broken reference in the same run would exit 1, and it is attributed to
/// nobody. Getting this wrong in the other direction is what makes a documentation
/// checker unusable, because every whitespace commit would start failing builds.
#[test]
fn a_formatting_only_change_to_a_target_is_advisory_and_never_a_verdict() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(
        root.join("README.md"),
        "# R\n\nSee [the source](target.txt).\n",
    )
    .unwrap();
    fs::write(root.join("target.txt"), "line one\nline two\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    // Whitespace only: a blank line between the two, and not one word touched.
    fs::write(root.join("target.txt"), "line one\n\nline two\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
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

    assert_eq!(code, 0, "reformatting a target does not fail a build");
    let payload = payload(&stdout);
    let findings = payload["findings"].as_array().unwrap();
    let raw = findings
        .iter()
        .find(|finding| finding["kind"] == "dependency-changed-subject-unchanged")
        .expect("the target moved under the document and the report says so");
    assert_eq!(
        raw["effective_disposition"], "warn",
        "advisory under enforce, which is the strictest profile there is"
    );
    assert_eq!(
        raw["attribution"], "not-applicable",
        "it accuses nobody: the bytes moved, and that is all anyone knows"
    );
    assert_eq!(payload["summary"]["findings"]["fail"], 0);
}

/// SHA-1 and SHA-256 repositories holding the same files must yield the same
/// facts. The object names differ, and that is all that may differ: every raw
/// content digest, every count, every finding, and every resolution is derived
/// from the bytes, not from how Git happens to address them. So this runs the
/// same content through both formats and compares. The whole summary must be
/// equal, the findings must land on the same kinds at the same paths, and each
/// document's content digest must agree while its object id visibly does not,
/// which is also the proof that the sha256 pipeline ran for real.
#[test]
fn a_sha256_repository_yields_the_same_facts_as_sha1() {
    let mut runs: Vec<serde_json::Value> = Vec::new();
    for format in ["sha1", "sha256"] {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init", "-q", &format!("--object-format={format}")]);
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

        let repo = root.to_str().unwrap_or_default().to_owned();
        let (code, stdout, stderr) = amiss(&[
            "check",
            "--repo",
            &repo,
            "--object-format",
            format,
            "--base",
            &base,
            "--candidate",
            &candidate,
            "--profile",
            "observe",
            "--format",
            "json",
        ]);
        assert_eq!((code, stderr.as_str()), (0, ""), "{format}");
        runs.push(payload(&stdout));
    }

    let (sha1, sha256) = (&runs[0], &runs[1]);
    assert_eq!(
        sha1["summary"], sha256["summary"],
        "every count is content-derived, so the summaries are one object"
    );

    let facts = |payload: &serde_json::Value| -> Vec<(String, String, String)> {
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|finding| {
                (
                    finding["kind"].as_str().unwrap().to_owned(),
                    finding["effective_disposition"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    finding["location"]["path"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    };
    assert_eq!(facts(sha1), facts(sha256));

    for row in sha1["documents"].as_array().unwrap() {
        let path = row["path"].as_str().unwrap();
        let twin = sha256["documents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|other| other["path"] == path)
            .unwrap();
        for side in ["base", "candidate"] {
            let (a, b) = (&row[side], &twin[side]);
            if a.is_null() {
                assert!(b.is_null(), "{path} {side}");
                continue;
            }
            assert_eq!(
                a["raw_digest"], b["raw_digest"],
                "{path} {side}: the content digest does not care how Git names the blob"
            );
            let (oid_a, oid_b) = (
                a["entry_oid"].as_str().unwrap(),
                b["entry_oid"].as_str().unwrap(),
            );
            assert_eq!((oid_a.len(), oid_b.len()), (40, 64), "{path} {side}");
        }
    }
}

/// V0 supplies no external controls, and the report must say so in the exact
/// vocabulary reserved for that: `none`, with no trust source and no digest.
/// The row this pins is not the absence, it is the labeling. A report that
/// described an unsupplied floor as anything but none, or dressed the local
/// process up as a verified sandbox, would be lending itself trust nobody
/// granted, and every consumer downstream of the report would inherit the lie.
#[test]
fn unsupplied_controls_report_none_and_claim_no_trust() {
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
        "--format",
        "json",
    ]);
    assert_eq!(code, 0);
    let payload = payload(&stdout);
    assert_eq!(
        payload["result"]["complete"], true,
        "none is a complete answer"
    );

    let controls = &payload["controls"];
    for control in ["organization_floor", "debt_snapshot", "waiver_bundle"] {
        assert_eq!(controls[control]["status"], "none", "{control}");
        assert_eq!(controls[control]["trust_source"], "none", "{control}");
        assert!(controls[control]["digest"].is_null(), "{control}");
    }
    assert_eq!(controls["execution_constraint"]["status"], "none");
    assert_eq!(controls["trusted_time_source"]["status"], "none");
    assert!(controls["base_repository_policy_digest"].is_null());
    assert!(controls["candidate_repository_policy_digest"].is_null());

    let sandbox = &controls["sandbox"];
    assert_eq!(sandbox["assurance"], "self-asserted");
    assert_eq!(sandbox["enforcement_source"], "local-process");
    assert!(
        sandbox["verification"].is_null(),
        "a local process does not get to claim it was verified"
    );
}

/// Writes `body` to the object store as a blob and returns its OID.
#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git_object(dir: &Path, body: &[u8]) -> String {
    use std::io::Write as _;
    let mut child = Command::new("git")
        .args(["hash-object", "-w", "--stdin"])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn git");
    child
        .stdin
        .as_mut()
        .expect("git stdin")
        .write_all(body)
        .expect("write blob body");
    let out = child.wait_with_output().expect("git output");
    assert!(
        out.status.success(),
        "hash-object: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("git output utf-8")
        .trim()
        .to_owned()
}

/// Stages one index entry whose path is exactly `path_bytes`, which may hold
/// bytes no shell or checkout would preserve, by feeding git raw index-info on
/// stdin.
#[expect(clippy::expect_used, reason = "test fixture helper")]
fn stage_raw_path(dir: &Path, blob: &str, path_bytes: &[u8]) {
    use std::io::Write as _;
    let mut spec = format!("100644 {blob}\t").into_bytes();
    spec.extend_from_slice(path_bytes);
    spec.push(0);
    let mut child = Command::new("git")
        .args(["update-index", "--add", "--index-info"])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn git");
    child
        .stdin
        .as_mut()
        .expect("git stdin")
        .write_all(&spec)
        .expect("write index-info");
    let out = child.wait_with_output().expect("git output");
    assert!(
        out.status.success(),
        "index-info: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A skip-worktree entry is still part of the staged snapshot; the bit only tells
/// the working tree not to bother materializing it. So in index mode its blob is
/// read from the index exactly like any other, its references resolve, and the
/// report both discloses the count of such entries and records that the candidate
/// was materialized from the index rather than from a commit. A scanner that read
/// the worktree instead would see nothing there and silently drop the document.
#[test]
fn a_skip_worktree_document_is_read_from_the_index_and_disclosed() {
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
    git(root, &["update-index", "--skip-worktree", "docs/guide.md"]);

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, stdout, stderr) = amiss(&[
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
    assert_eq!((code, stderr.as_str()), (0, ""));
    let payload = payload(&stdout);
    assert_eq!(payload["evaluation"]["materialization"], "index");
    assert_eq!(
        payload["evaluation"]["skip_worktree_paths"], 1,
        "the one skip-worktree entry is counted"
    );
    assert_eq!(
        payload["summary"]["references"]["missing"], 0,
        "its reference resolves, so its bytes were read from the index"
    );
    let guide = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == "docs/guide.md")
        .expect("the skip-worktree document is in the set");
    assert_eq!(
        guide["candidate"]["content_availability"], "available",
        "the blob was materialized from the index, not skipped"
    );
}

/// A repository path is untrusted bytes, and the human projection is a place those
/// bytes could become terminal control sequences, a forged workflow command, or a
/// second log line. The `human-atom-v1` law says every scalar outside printable
/// ASCII becomes a `\uXXXX` escape, so an ESC, a carriage return, and a bell in a
/// document's own path all leave the renderer inert. The escaping law has unit
/// tests; this drives a genuinely hostile path all the way through the binary and
/// reads the bytes it actually printed, because a law is only worth what the
/// product does with it.
#[test]
fn a_hostile_document_path_is_rendered_inert_and_round_trips_in_json() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    // ESC, an ANSI colour run, a forged GitHub Actions command, a bell, and a
    // carriage return, all valid UTF-8 and all valid in a RepoPath.
    let hostile = "docs/\u{1b}[31m::error::forged\u{7}\u{d}.md";
    let blob = git_object(root, b"# X\n\n[b](nowhere.md)\n");
    stage_raw_path(root, &blob, hostile.as_bytes());
    let tree = git(root, &["write-tree"]).trim().to_owned();
    let candidate = git(root, &["commit-tree", &tree, "-p", &base, "-m", "hostile"])
        .trim()
        .to_owned();

    let repo = root.to_str().unwrap_or_default().to_owned();
    let (code, human, _stderr) = amiss(&[
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
        "human",
    ]);
    assert_eq!(code, 0, "a hostile path is still an ordinary document");
    for raw in [0x1b_u8, 0x0d, 0x07] {
        assert!(
            !human.contains(&raw),
            "raw control byte {raw:#04x} reached the human output"
        );
    }
    let human_text = String::from_utf8(human).expect("human output is utf-8");
    assert!(
        human_text.contains("\\u001b") && human_text.contains("\\u000d"),
        "the control bytes are present, but only as escapes"
    );

    let (code, json, _stderr) = amiss(&[
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
    assert_eq!(code, 0);
    let payload = payload(&json);
    let paths: Vec<&str> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["path"].as_str())
        .collect();
    assert!(
        paths.contains(&hostile),
        "json carries the exact bytes as a string, losing nothing: {paths:?}"
    );
}
