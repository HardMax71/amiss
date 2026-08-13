use std::fs;

use crate::support;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn pair_with_external_urls() -> amiss_fixtures::CommitPair {
    amiss_fixtures::commit_pair(
        &[(
            "docs/a.md",
            "[kept](https://kept.example/k) [old](https://old.example/g)\n",
        )],
        &[(
            "docs/a.md",
            "[kept](https://kept.example/k) [new](https://new.example/n)\n",
        )],
    )
    .unwrap()
}

fn checked_report(pair: &amiss_fixtures::CommitPair) -> Vec<u8> {
    let (code, stdout, stderr) = support::amiss(&[
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
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(!stdout.is_empty());
    stdout
}

#[test]
fn the_plan_lists_the_delta_and_binds_the_report_digest() {
    let pair = pair_with_external_urls();
    let report_bytes = checked_report(&pair);
    let report: serde_json::Value = serde_json::from_slice(&report_bytes).unwrap();
    let report_path = format!("{}/report.json", pair.repo);
    fs::write(&report_path, &report_bytes).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-plan",
        "--report",
        &report_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let plan: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(plan["schema"], "amiss/external-plan-envelope");
    assert_eq!(plan["payload"]["schema"], "amiss/external-plan-payload");
    assert_eq!(
        plan["payload"]["report"]["payload_digest"], report["payload_digest"],
        "the plan binds the digest of the report it read"
    );
    assert_eq!(
        plan["payload"]["introduced"],
        serde_json::json!([{
            "destination": "https://new.example/n",
            "documents": ["docs/a.md"],
            "scheme": "https",
        }]),
    );
    assert_eq!(
        plan["payload"]["removed"],
        serde_json::json!([{
            "destination": "https://old.example/g",
            "documents": ["docs/a.md"],
            "scheme": "https",
        }]),
    );
    assert_eq!(plan["payload"]["retained_count"], 1);
}

#[test]
fn a_tampered_report_is_refused_with_exit_two() {
    let pair = pair_with_external_urls();
    let report_bytes = checked_report(&pair);
    let tampered = String::from_utf8(report_bytes)
        .unwrap()
        .replace("https://old.example/g", "https://old.example/x");
    let report_path = format!("{}/tampered.json", pair.repo);
    fs::write(&report_path, tampered).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-plan",
        "--report",
        &report_path,
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("does not match its recorded digest"),
        "the refusal names the digest defect: {stderr}"
    );
}

#[test]
fn the_human_projection_summarizes_the_delta() {
    let pair = pair_with_external_urls();
    let report_bytes = checked_report(&pair);
    let report_path = format!("{}/report.json", pair.repo);
    fs::write(&report_path, &report_bytes).unwrap();

    let (code, stdout, stderr) = support::amiss(&["external-plan", "--report", &report_path]);
    let stdout = String::from_utf8(stdout).unwrap();
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        stdout,
        "amiss external-plan: introduced 1 removed 1 retained 1\n\
         introduced https://new.example/n in 1 documents\n"
    );
}

#[test]
fn the_grammar_closes_the_plan_form() {
    for argv in [
        ["external-plan"].as_slice(),
        &["external-plan", "--report", ""],
        &["external-plan", "--report", "a.json", "--repo", "."],
        &["external-plan", "--report", "a.json", "--report", "b.json"],
        &["check", "--report", "a.json"],
    ] {
        let (code, _stdout, stderr) = support::amiss(argv);
        assert_eq!(code, 2, "{argv:?} must be refused");
        assert!(
            stderr.contains("INVALID_INVOCATION"),
            "{argv:?} names the code: {stderr}"
        );
    }
    // A machine projection carries the refusal on its own channel.
    let (code, stdout, _stderr) =
        support::amiss(&["external-plan", "--report", "a.json", "--format", "sarif"]);
    assert_eq!(code, 2);
    assert!(
        String::from_utf8_lossy(&stdout).contains("INVALID_INVOCATION"),
        "the sarif refusal names the code"
    );
}

#[test]
fn an_unreadable_report_is_a_plain_refusal() {
    let (code, stdout, stderr) =
        support::amiss(&["external-plan", "--report", "/nonexistent/report.json"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("is unreadable"), "{stderr}");
}
