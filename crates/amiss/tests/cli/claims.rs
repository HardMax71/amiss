use crate::support::{amiss, claim_fixture, payload};

fn check(fx: &amiss_fixtures::CommitPair, profile: &str) -> (i32, Vec<u8>) {
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
        profile,
        "--format",
        "json",
    ]);
    assert_eq!(stderr, "");
    (code, stdout)
}

/// The binary end to end: a claim the candidate breaks warns under observe
/// and fails under enforce, and the finding row carries the claim evidence
/// with the kind-level projections the wire fixes per kind.
#[test]
fn a_broken_value_claim_warns_under_observe_and_fails_under_enforce() {
    let fx = claim_fixture();

    let (code, stdout) = check(&fx, "observe");
    assert_eq!(code, 0, "a warned claim does not block observe");
    let report = payload(&stdout);
    assert_eq!(report["result"]["status"], "pass");
    assert_eq!(report["summary"]["governed_claims"], 1);
    assert_eq!(report["summary"]["unattested_claims"], 1);
    let findings = report["findings"].as_array().unwrap();
    let row = findings
        .iter()
        .find(|row| row["kind"] == "claim-broken")
        .unwrap();
    assert_eq!(row["effective_disposition"], "warn");
    assert_eq!(row["attribution"], "not-applicable");
    assert_eq!(row["coverage_requirement"], "control-plane");
    assert_eq!(row["evidence_class"], "deterministic-structural");
    assert_eq!(row["invariant_class"], "ratcheted");
    let scope = &row["key_input"]["scope"];
    assert_eq!(scope["kind"], "control");
    assert_eq!(scope["control_path"], "docs/claims.md");
    assert_eq!(scope["rule_id"], "claim/value/subject-line");
    let evidence = &row["candidate_fact"]["evidence"];
    assert_eq!(evidence["kind"], "claim");
    assert_eq!(evidence["claim_kind"], "value");
    assert_eq!(evidence["name"], "subject-line");
    assert_eq!(evidence["target_path"], "subject.txt");
    assert_eq!(evidence["line"], 1);
    assert_eq!(evidence["observed"], "line-differs");
    assert!(
        evidence["observed_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:"))
    );

    let (code, stdout) = check(&fx, "enforce");
    assert_eq!(code, 1, "the same claim blocks enforce");
    let report = payload(&stdout);
    assert_eq!(report["result"]["status"], "fail");
    let findings = report["findings"].as_array().unwrap();
    let row = findings
        .iter()
        .find(|row| row["kind"] == "claim-broken")
        .unwrap();
    assert_eq!(row["effective_disposition"], "fail");
}

/// The human projection groups the broken claim into one fix item without
/// leaking the internal kind name.
#[test]
fn a_broken_claim_lands_as_one_fix_item_in_human_output() {
    let fx = claim_fixture();
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
        "observe",
    ]);
    assert_eq!(code, 0);
    let text = String::from_utf8_lossy(&stdout);
    assert!(
        text.starts_with("amiss: pass (fix 1, check 0, existing 0, errors 0, exit 0)"),
        "got: {text}"
    );
    assert!(
        text.contains("Fix target - affected places 1"),
        "the claim finding groups into one untargeted fix item: {text}"
    );
    assert!(
        !text.contains("claim-broken"),
        "internal finding kinds stay out of the focused human projection: {text}"
    );
}
