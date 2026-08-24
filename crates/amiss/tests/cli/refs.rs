#![expect(
    clippy::expect_used,
    reason = "integration assertions over fixed fixtures"
)]

use std::fs;
use std::process::{Command, Stdio};

use crate::support::{amiss, fixture};

fn report(profile: &str) -> (amiss_fixtures::CommitPair, String, i32) {
    let pair = fixture();
    let (code, bytes, stderr) = amiss(&[
        "check",
        "--repo",
        &pair.repo,
        "--object-format",
        "sha1",
        "--base",
        &pair.base,
        "--candidate",
        &pair.candidate,
        "--profile",
        profile,
        "--format",
        "json",
    ]);
    assert_eq!(stderr, "");
    let path = format!("{}/report.json", pair.repo);
    fs::write(&path, bytes).expect("write report fixture");
    (pair, path, code)
}

#[test]
fn json_returns_exact_candidate_occurrences_without_revising_the_verdict() {
    let (_pair, path, report_code) = report("enforce");
    assert_eq!(report_code, 1, "the source report is blocking");

    let (code, stdout, stderr) = amiss(&[
        "refs",
        "--report",
        &path,
        "--target",
        "docs/guide.md",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let rows: serde_json::Value = serde_json::from_slice(&stdout).expect("strict JSON result");
    let rows = rows.as_array().expect("occurrence array");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["document"], "README");
    assert_eq!(row["intent"]["repository_path"], "docs/guide.md");
    assert_eq!(row["resolution"]["target"]["path"], "docs/guide.md");
    assert_eq!(row["source_construct"], "markdown-inline-link");
    assert!(row["observation_id"].as_str().is_some());
    assert!(row["source_span"]["start_line"].as_u64().is_some());

    let (empty_code, empty, empty_stderr) = amiss(&[
        "refs",
        "--report",
        &path,
        "--target",
        "docs/absent.md",
        "--format",
        "json",
    ]);
    assert_eq!((empty_code, empty_stderr.as_str()), (0, ""));
    assert_eq!(empty, b"[]\n");
}

#[test]
fn unresolved_intents_and_human_locations_are_queryable() {
    let (_pair, path, _report_code) = report("observe");
    let (code, stdout, stderr) = amiss(&["refs", "--report", &path, "--target", "docs/missing.md"]);
    let stdout = String::from_utf8(stdout).expect("human output is UTF-8");
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(
        stdout.starts_with(
            "amiss refs: target \"docs/missing.md\" candidate occurrences 1\nreference \"docs/guide.md\":"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("\"markdown-inline-link\" \"missing\" \"sha256:"));
}

#[test]
fn untrusted_and_incomplete_reports_are_refused() {
    let (pair, path, _report_code) = report("observe");
    let tampered = fs::read_to_string(&path)
        .expect("read report")
        .replace("docs/missing.md", "docs/otherxx.md");
    let tampered_path = format!("{}/tampered.json", pair.repo);
    fs::write(&tampered_path, tampered).expect("write tampered report");

    let absent = format!("{}/absent", pair.repo);
    let (incomplete_code, incomplete, incomplete_stderr) = amiss(&[
        "check",
        "--repo",
        &absent,
        "--object-format",
        "sha1",
        "--base",
        &pair.base,
        "--candidate",
        &pair.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((incomplete_code, incomplete_stderr.as_str()), (2, ""));
    let incomplete_path = format!("{}/incomplete.json", pair.repo);
    fs::write(&incomplete_path, incomplete).expect("write incomplete report");

    for (input, reason) in [
        (tampered_path.as_str(), "does not match its recorded digest"),
        (incomplete_path.as_str(), "report is incomplete"),
    ] {
        let (code, stdout, stderr) = amiss(&[
            "refs",
            "--report",
            input,
            "--target",
            "docs/guide.md",
            "--format",
            "json",
        ]);
        assert_eq!(code, 2, "{input}");
        assert!(stdout.is_empty(), "{input}");
        assert!(stderr.contains(reason), "{input}: {stderr}");
    }
}

#[test]
fn the_grammar_closes_the_query_form() {
    for argv in [
        ["refs", "--report", "report.json"].as_slice(),
        &[
            "refs",
            "--report",
            "report.json",
            "--target",
            "a.md",
            "--target-bytes-hex",
            "612e6d64",
        ],
        &[
            "refs",
            "--report",
            "report.json",
            "--target-bytes-hex",
            "FF",
        ],
        &["refs", "--report", "report.json", "--target-bytes-hex", "f"],
        &[
            "refs",
            "--report",
            "report.json",
            "--target",
            "a.md",
            "--repo",
            ".",
        ],
        &[
            "external-plan",
            "--report",
            "report.json",
            "--target",
            "a.md",
        ],
    ] {
        let (code, _stdout, stderr) = amiss(argv);
        assert_eq!(code, 2, "{argv:?}");
        assert!(stderr.contains("INVALID_INVOCATION"), "{argv:?}: {stderr}");
    }
}

#[test]
fn a_closed_pipe_preserves_query_success() {
    let (_pair, path, _report_code) = report("observe");
    for format in ["human", "json"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_amiss"))
            .args([
                "refs",
                "--report",
                &path,
                "--target",
                "docs/guide.md",
                "--format",
                format,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn amiss refs");
        drop(child.stdout.take());
        let output = child.wait_with_output().expect("collect amiss refs");
        assert_eq!(output.status.code(), Some(0), "{format}");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "", "{format}");
    }
}
