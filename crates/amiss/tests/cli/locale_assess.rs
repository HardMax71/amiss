use crate::support;

const PLAN: &str = "../../spec/examples/locale-coverage-plan.json";
const EVIDENCE: &str = "../../spec/examples/locale-coverage-evidence.json";

#[test]
fn the_published_pair_assesses_to_a_matched_coverage_result() {
    let (code, stdout, stderr) =
        support::amiss(&["locale-assess", "--plan", PLAN, "--evidence", EVIDENCE]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "amiss locale-assess: matched missing 0 orphaned 0 fallbacks 1 lineage 1\n"
    );

    let (code, stdout, stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        PLAN,
        "--evidence",
        EVIDENCE,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let document: serde_json::Value = serde_json::from_slice(&stdout).expect("one JSON document");
    assert_eq!(
        document["schema"],
        serde_json::json!("amiss/locale-coverage-assessment-envelope")
    );
    assert_eq!(document["payload"]["verdict"], serde_json::json!("matched"));
}

#[test]
fn evidence_bound_to_another_plan_is_unproven_rather_than_refused() {
    let scratch = tempfile::tempdir().expect("a scratch root");
    let foreign = scratch.path().join("evidence.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(EVIDENCE).expect("the example")).expect("JSON");
    document["payload"]["plan_payload_digest"] =
        serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    std::fs::write(&foreign, serde_json::to_vec(&document).expect("bytes")).expect("write");

    let (code, _stdout, stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        PLAN,
        "--evidence",
        foreign.to_str().expect("a utf-8 path"),
    ]);
    assert_eq!(code, 2, "a rebound payload no longer reproduces its digest");
    assert!(stderr.contains("digest does not match"), "{stderr}");
}

#[test]
fn the_grammar_closes_the_coverage_form() {
    for argv in [
        ["locale-assess"].as_slice(),
        &["locale-assess", "--plan", PLAN],
        &["locale-assess", "--evidence", EVIDENCE],
        &[
            "locale-assess",
            "--plan",
            PLAN,
            "--evidence",
            EVIDENCE,
            "--repo",
            ".",
        ],
    ] {
        let (code, _stdout, stderr) = support::amiss(argv);
        assert_eq!(code, 2, "{argv:?} must be refused");
        assert!(
            stderr.contains("INVALID_INVOCATION"),
            "{argv:?} names the code: {stderr}"
        );
    }
    // A machine projection carries the refusal on its own channel.
    let (code, stdout, _stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        PLAN,
        "--evidence",
        EVIDENCE,
        "--format",
        "sarif",
    ]);
    assert_eq!(code, 2);
    assert!(
        String::from_utf8_lossy(&stdout).contains("INVALID_INVOCATION"),
        "the sarif refusal names the code"
    );
}

#[test]
fn an_unreadable_document_is_a_plain_refusal() {
    let (code, stdout, stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        "no-such-plan.json",
        "--evidence",
        EVIDENCE,
    ]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("is unreadable"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn an_oversized_plan_is_refused_without_reading_it_all() {
    let (code, _stdout, stderr) = support::amiss(&[
        "locale-assess",
        "--plan",
        "/dev/zero",
        "--evidence",
        EVIDENCE,
    ]);
    assert_eq!(code, 2);
    assert!(
        stderr.contains("larger than a coverage plan can be"),
        "{stderr}"
    );
}
