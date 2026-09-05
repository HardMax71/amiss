use std::fs;
use std::process::Command;

use tempfile::TempDir;

use crate::support::{amiss, fixture, git, payload};

#[test]
fn a_limit_crossing_names_the_resource_and_both_numbers() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("blob-content"), "# X\n").unwrap();
    let blob = git(root, &["hash-object", "-w", "--", "blob-content"])
        .trim()
        .to_owned();
    let long = format!("{}/x.md", vec!["a".repeat(200); 25].join("/"));
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},{long}"),
        ],
    );
    let tree = git(root, &["write-tree"]).trim().to_owned();
    let candidate = git(root, &["commit-tree", &tree, "-p", &base, "-m", "long"])
        .trim()
        .to_owned();

    let repo = amiss_fixtures::path_arg(root);
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
    assert_eq!(code, 2, "a crossed ceiling is never a trustworthy result");
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.contains("error git RESOURCE_LIMIT_EXCEEDED - raw-path-bytes 4096/"),
        "the crossing names the resource and both numbers: {text}"
    );
}

/// Piping either output through `head` closes stdout mid-print. The narration
/// stops; the verdict does not: the exit class reports the run, never whether
/// anyone kept reading, and a closed pipe is not a panic and not an error.
#[test]
fn a_closed_pipe_ends_the_narration_and_not_the_verdict() {
    let fx = fixture();
    for format in ["human", "json"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_amiss"))
            .args([
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
                format,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn amiss");
        drop(child.stdout.take());
        let output = child.wait_with_output().expect("collect the run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{format}: the verdict survives the closed pipe"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("panicked"),
            "{format}: a closed pipe is not a panic: {stderr}"
        );
        assert!(
            !stderr.contains("REPORT_CONSTRUCTION_FAILED"),
            "{format}: a consumer leaving is not a construction failure: {stderr}"
        );
    }
}

#[test]
fn report_emission_rejects_a_read_only_destination() {
    use amiss_wire::report::model::ReportEnvelope;

    let report: ReportEnvelope = serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut output = std::io::BufWriter::new(fs::File::open(file.path()).unwrap());
    assert!(amiss_wire::report::emit_report(&report, &mut output).is_err());
    assert_eq!(file.as_file().metadata().unwrap().len(), 0);
}

#[test]
fn a_clean_observe_run_passes_with_a_complete_report() {
    let fx = fixture();
    let (code, stdout, stderr) = amiss(&[
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
    let fx = fixture();
    let (code, stdout, _stderr) = amiss(&[
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
    let missing = payload["findings"]
        .as_array()
        .and_then(|findings| {
            findings
                .iter()
                .find(|finding| finding["kind"] == "explicit-target-missing")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        missing["description"],
        amiss_wire::report::FindingKind::ExplicitTargetMissing.meaning(),
        "every finding row carries its kind's fixed description"
    );
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
    let fx = fixture();
    let root = fx.root();
    fs::write(root.join("docs/staged.md"), "# Staged\n\n[up](guide.md)\n").unwrap_or_default();
    git(root, &["add", "docs/staged.md"]);
    fs::write(
        root.join("docs/staged.md"),
        "worktree drift with [broken](nowhere.md)\n",
    )
    .unwrap_or_default();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.candidate,
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

    let repo = amiss_fixtures::path_arg(root);
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
    ]);
    assert_eq!(code, 2);
    let human = String::from_utf8_lossy(&human);
    assert!(
        human.starts_with("amiss: scan failed (errors "),
        "an incomplete comparison is never presented as zero feedback: {human}"
    );
    assert!(!human.contains("fix 0, check 0"), "{human}");
    assert!(
        human.contains(&format!(
            "note DOCUMENT_INVALID: {}",
            amiss_wire::report::AnalysisErrorCode::DocumentInvalid.meaning()
        )),
        "an exit-2 log says how to unblock the run: {human}"
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

    let repo = amiss_fixtures::path_arg(root);
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
        "enforce",
    ]);
    assert_eq!(code, 0);
    let human = String::from_utf8_lossy(&human);
    assert!(
        human.starts_with("amiss: pass (fix 0, check 1, existing 0, errors 0, exit 0)"),
        "{human}"
    );
    assert!(
        human.contains("Check target \"target.txt\" affected places 1"),
        "checks stay visible without being called fixes: {human}"
    );
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

        let repo = amiss_fixtures::path_arg(root);
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

    let repo = amiss_fixtures::path_arg(root);
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
