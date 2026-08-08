use crate::support::{amiss, fixture, payload};

/// The Code Quality lane is a projection, so every claim it makes is checked
/// against the canonical report from the same evaluation. Returns the issues
/// for assertions a single profile owns.
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "projection assertions over test-built reports"
)]
fn mirrored_issues(profile: &str) -> Vec<serde_json::Value> {
    let fx = fixture();
    let args = [
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
        profile,
        "--format",
        "codequality",
    ];
    let (_code, first, stderr) = amiss(&args);
    assert_eq!(stderr, "");
    let (_again, second, _stderr) = amiss(&args);
    assert_eq!(first, second, "identical inputs, identical artifact bytes");

    let wire_args = [
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
        profile,
        "--format",
        "json",
    ];
    let (_wire_code, wire, _wire_stderr) = amiss(&wire_args);
    let findings = payload(&wire)["findings"].as_array().unwrap().clone();
    let issues: Vec<serde_json::Value> = serde_json::from_slice(&first).unwrap();
    let mut canonical = serde_json::to_vec(&issues).unwrap();
    canonical.push(b'\n');
    assert_eq!(
        first, canonical,
        "the artifact is one canonical line, members in sorted order"
    );
    assert_eq!(issues.len(), findings.len());
    for (issue, finding) in issues.iter().zip(&findings) {
        assert_eq!(issue.get("check_name"), finding.get("kind"));
        assert_eq!(issue.get("fingerprint"), finding.get("finding_key"));
        assert_eq!(issue.get("description"), finding.get("description"));
        let expected = match finding["effective_disposition"].as_str().unwrap() {
            "fail" => "major",
            "warn" => "minor",
            _ => "info",
        };
        assert_eq!(issue["severity"].as_str().unwrap(), expected);
        if finding
            .pointer("/location/path")
            .is_some_and(serde_json::Value::is_string)
        {
            assert_eq!(
                issue.pointer("/location/path"),
                finding.pointer("/location/path"),
            );
        }
        match finding.pointer("/location/span/start_line") {
            Some(line) => assert_eq!(issue.pointer("/location/lines/begin"), Some(line)),
            None => assert_eq!(
                issue.pointer("/location/lines/begin"),
                Some(&serde_json::json!(1)),
            ),
        }
    }
    issues
}

#[test]
fn the_code_quality_projection_mirrors_the_report_and_stays_deterministic() {
    let issues = mirrored_issues("enforce");
    assert!(
        issues.iter().any(|issue| issue["severity"] == "major"),
        "an enforce run projects its blocking rows as major"
    );
}

/// Under observe nothing blocks, so no issue may claim otherwise.
#[test]
fn an_observe_run_projects_no_major_issue() {
    let issues = mirrored_issues("observe");
    assert!(!issues.is_empty());
    assert!(issues.iter().all(|issue| issue["severity"] != "major"));
}

/// The format has no shape for a refusal, so a rejected machine invocation
/// answers with a valid empty artifact and the exit class carries the truth.
#[test]
fn a_code_quality_refusal_is_an_empty_artifact() {
    let (code, refusal, stderr) = amiss(&["check", "--format", "codequality"]);
    assert_eq!((code, stderr.as_str()), (2, ""));
    let issues: Vec<serde_json::Value> = serde_json::from_slice(&refusal).unwrap();
    assert!(issues.is_empty());
}
