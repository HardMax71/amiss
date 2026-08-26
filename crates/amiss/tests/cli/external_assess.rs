use std::fs;

use crate::support;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn planned_pair() -> (amiss_fixtures::CommitPair, String, serde_json::Value) {
    let pair = amiss_fixtures::commit_pair(
        &[("docs/a.md", "[kept](https://kept.example/k)\n")],
        &[(
            "docs/a.md",
            "[kept](https://kept.example/k) [new](https://new.example/n)\n",
        )],
    )
    .unwrap();
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
    let report_path = format!("{}/report.json", pair.repo);
    fs::write(&report_path, &stdout).unwrap();
    let (code, plan_bytes, stderr) = support::amiss(&[
        "external-plan",
        "--report",
        &report_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let plan: serde_json::Value = serde_json::from_slice(&plan_bytes).unwrap();
    let plan_path = format!("{}/plan.json", pair.repo);
    fs::write(&plan_path, &plan_bytes).unwrap();
    (pair, plan_path, plan)
}

fn evidence_json(plan: &serde_json::Value, status: u16, method: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": "amiss/external-evidence",
        "plan_payload_digest": plan["payload_digest"],
        "producer": {"name": "curl-recipe", "version": "0"},
        "rows": [{
            "kind": "http-probe",
            "destination": "https://new.example/n",
            "method": method,
            "status": status,
            "checked_at": "2026-08-14T00:00:00Z",
        }],
    })
}

#[test]
fn the_chain_judges_an_introduced_destination() {
    let (pair, plan_path, plan) = planned_pair();
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(
        &evidence_path,
        serde_json::to_string(&evidence_json(&plan, 410, "get")).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let assessment: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(assessment["schema"], "amiss/external-assessment-envelope");
    assert_eq!(
        assessment["payload"]["verdicts"],
        serde_json::json!([{
            "destination": "https://new.example/n",
            "documents": ["docs/a.md"],
            "reason": "gone",
            "verdict": "refuted",
        }]),
    );
    assert_eq!(
        assessment["payload"]["subject"]["plan_payload_digest"],
        plan["payload_digest"],
    );
    assert_eq!(
        assessment["payload"]["subject"]["report_payload_digest"],
        plan["payload"]["report"]["payload_digest"],
    );
}

#[test]
fn evidence_for_a_foreign_plan_is_refused() {
    let (pair, plan_path, plan) = planned_pair();
    let mut foreign = evidence_json(&plan, 200, "head");
    foreign["plan_payload_digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(&evidence_path, serde_json::to_string(&foreign).unwrap()).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("binds another plan"), "{stderr}");
}

#[test]
fn the_human_projection_windows_the_refuted() {
    let (pair, plan_path, plan) = planned_pair();
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(
        &evidence_path,
        serde_json::to_string(&evidence_json(&plan, 404, "get")).unwrap(),
    )
    .unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "amiss external-assess: refuted 1 unproven 0 reachable 0\n\
         refuted \"https://new.example/n\" (gone)\n"
    );
}

#[test]
fn the_human_projection_suggests_only_a_proved_permanent_retarget() {
    let (pair, plan_path, plan) = planned_pair();
    let mut evidence = evidence_json(&plan, 200, "head");
    evidence["rows"][0]["final_destination"] = serde_json::json!("https://current.example/n");
    evidence["rows"][0]["redirect_chain_permanent"] = serde_json::json!(true);
    let evidence_path = format!("{}/evidence.json", pair.repo);
    fs::write(&evidence_path, serde_json::to_string(&evidence).unwrap()).unwrap();

    let (code, stdout, stderr) = support::amiss(&[
        "external-assess",
        "--plan",
        &plan_path,
        "--evidence",
        &evidence_path,
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        "amiss external-assess: refuted 0 unproven 0 reachable 1\n\
         retarget suggestion \"https://new.example/n\" -> \"https://current.example/n\"\n"
    );
}

#[test]
fn the_grammar_closes_the_assessment_form() {
    for argv in [
        ["external-assess"].as_slice(),
        &["external-assess", "--plan", "p.json"],
        &["external-assess", "--evidence", "e.json"],
        &[
            "external-assess",
            "--plan",
            "p.json",
            "--evidence",
            "e.json",
            "--report",
            "r.json",
        ],
        &["external-assess", "--plan", "", "--evidence", "e.json"],
        &["check", "--plan", "p.json"],
        &[
            "external-plan",
            "--report",
            "r.json",
            "--evidence",
            "e.json",
        ],
    ] {
        let (code, _stdout, stderr) = support::amiss(argv);
        assert_eq!(code, 2, "{argv:?} must be refused");
        assert!(
            stderr.contains("INVALID_INVOCATION"),
            "{argv:?} names the code: {stderr}"
        );
    }
}
