use amiss_scan::pipeline::staged_index;
use amiss_wire::controls::Profile;

use crate::support::{
    debt_input, debt_json, engine, fixture, floor_input, payload, shell, structural_evidence,
    time_input, waiver_input, waiver_json,
};

#[test]
fn overlapping_valid_exceptions_are_fatal_and_apply_neither() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-08-01T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["result"]["status"], "incomplete");
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "EXCEPTION_OVERLAP")
    );
    let structural = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(structural["debt"], serde_json::Value::Null);
    assert_eq!(structural["waiver"], serde_json::Value::Null);
    assert_eq!(structural["effective_disposition"], "fail");
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 0);
    assert_eq!(report["summary"]["findings"]["waived"], 0);
}

#[test]
fn resolved_finding_is_not_an_exception_target() {
    let fx = fixture("[note](note.md)\n");
    let standing = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&standing);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-08-01T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(report["controls"]["debt_snapshot"]["status"], "verified");
    assert_eq!(report["controls"]["waiver_bundle"]["status"], "verified");
    let structural = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .expect("the base-only projection remains visible");
    assert_eq!(structural["attribution"], "resolved");
    assert_eq!(structural["candidate_fact"], serde_json::Value::Null);
    assert_eq!(structural["candidate_fact_digest"], serde_json::Value::Null);
    assert_eq!(structural["effective_disposition"], "record");
    assert_eq!(structural["debt"], serde_json::Value::Null);
    assert_eq!(structural["waiver"], serde_json::Value::Null);
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 0);
    assert_eq!(report["summary"]["findings"]["waived"], 0);
    assert!(!report["findings"].as_array().unwrap().iter().any(|row| {
        matches!(
            row["kind"].as_str(),
            Some("debt-expired" | "debt-worsened" | "waiver-invalid")
        )
    }));
    assert!(
        !report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "EXCEPTION_OVERLAP")
    );
}

#[test]
fn expiry_bearing_controls_require_a_trusted_instant() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["controls"]["status"], "unavailable");
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["invalid-external-control"])
    );
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "TRUSTED_TIME_INVALID")
    );
}

#[test]
fn the_statement_binding_must_identify_the_authenticated_run() {
    let fx = fixture("see [gone](missing.md)\n");
    let mut setup = shell(Profile::Observe);
    let mut time = time_input(&fx);
    time.provider_run_attempt = 3;
    setup.time = Some(time);
    let report = payload(&fx, &setup);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["invalid-external-control"])
    );
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "TRUSTED_TIME_INVALID")
    );
}

#[test]
fn index_mode_rejects_tree_bound_exceptions() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    let built = staged_index(&fx.repo, &engine(), None, &setup, &fx.base);
    let envelope: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let report = &envelope["payload"];

    assert_eq!(built.exit_code, 2);
    assert_eq!(report["controls"]["status"], "unavailable");
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["control-binding-mismatch"])
    );
}
