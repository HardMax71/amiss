use amiss_wire::controls::Profile;

use crate::support::{
    assert_global_location, fixture, floor_input, payload, shell, structural_evidence, time_input,
    waiver_input, waiver_json,
};

#[test]
fn a_valid_waiver_changes_fail_to_warn() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
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

    assert_eq!(report["result"]["status"], "pass");
    assert_eq!(report["exit_code"], 0);
    assert_eq!(report["controls"]["waiver_bundle"]["status"], "verified");
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(finding["effective_disposition"], "warn");
    let last = finding["policy_trace"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["source"], "waiver-bundle");
    assert_eq!(last["rule_id"], "waiver/acme/release-window");
    assert_eq!(last["before"], "fail");
    assert_eq!(last["after"], "warn");
    assert_eq!(finding["waiver"]["waiver_id"], "acme/release-window");
    assert_eq!(finding["waiver"]["issuer"], "team:release-engineering");
    assert_eq!(report["summary"]["findings"]["waived"], 1);
}

#[test]
fn waiver_defects_emit_invalid_rows_without_suppression() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();

    let unauthorized = waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "team:docs-platform",
        "2026-08-01T00:00:00Z",
    );
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
    setup.waiver = Some(waiver_input(&unauthorized));
    let report = payload(&fx, &setup);

    assert_eq!(report["result"]["status"], "fail");
    assert_eq!(report["exit_code"], 1);
    assert_eq!(report["summary"]["findings"]["waived"], 0);
    for finding in report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "waiver-invalid")
    {
        assert_global_location(finding);
    }
    let rules: Vec<String> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "waiver-invalid")
        .map(|row| {
            row["key_input"]["scope"]["rule_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(
        rules.contains(&"waiver/acme/release-window/issuer".to_owned()),
        "issuer off the floor allow-list; issuer equals owner"
    );
    assert!(rules.contains(&"waiver/acme/release-window/same-owner".to_owned()));
    let structural = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(structural["effective_disposition"], "fail");

    let expired = waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-07-10T00:00:00Z",
    );
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
    setup.waiver = Some(waiver_input(&expired));
    let report = payload(&fx, &setup);
    for finding in report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "waiver-invalid")
    {
        assert_global_location(finding);
    }
    let rules: Vec<String> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "waiver-invalid")
        .map(|row| {
            row["key_input"]["scope"]["rule_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(rules, vec!["waiver/acme/release-window/expired".to_owned()]);
}

/// The waiver bundle is the only control that turns a failing finding into a
/// warning, so every binding on it carries the whole weight of the suppression. An
/// attacker who can present a bundle bound to some other repository, some other
/// branch, or some other organization floor, and have it honoured here, has a
/// general-purpose off switch for this scanner. The bundle therefore binds on all
/// of them plus its own issuance instant, and a bundle that fails any one of those
/// is not a weaker bundle: the controls go unavailable, the run is incomplete, and
/// the exit is 2. Every branch of that binding was written and none was tested.
#[test]
fn a_waiver_bundle_bound_to_anything_else_verifies_nothing_and_waives_nothing() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let valid = waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-08-01T00:00:00Z",
    );
    let other_floor = format!("sha256:{}", "0".repeat(64));

    let cases = [
        (
            "another owner",
            valid.replace(r#""owner": "acme""#, r#""owner": "evil""#),
        ),
        (
            "another repository",
            valid.replace(r#""name": "docs""#, r#""name": "widgets""#),
        ),
        (
            "another branch",
            valid.replace(
                r#""ref": "refs/heads/main""#,
                r#""ref": "refs/heads/attacker""#,
            ),
        ),
        ("another floor", valid.replace(&floor_digest, &other_floor)),
        (
            "issued after the trusted instant",
            valid.replace(
                r#""created_at": "2026-07-03T00:00:00Z""#,
                r#""created_at": "2026-09-01T00:00:00Z""#,
            ),
        ),
    ];

    for (bound_to, doc) in cases {
        assert_ne!(
            doc, valid,
            "{bound_to}: the fixture did not actually change"
        );
        let mut setup = shell(Profile::Enforce);
        setup.time = Some(time_input(&fx));
        setup.waiver = Some(waiver_input(&doc));
        let report = payload(&fx, &setup);

        assert_eq!(report["exit_code"], 2, "{bound_to}");
        assert_eq!(report["result"]["status"], "incomplete", "{bound_to}");
        assert_eq!(report["controls"]["status"], "unavailable", "{bound_to}");
        assert_eq!(
            report["controls"]["reasons"],
            serde_json::json!(["control-binding-mismatch"]),
            "{bound_to}"
        );
        assert_eq!(
            report["summary"]["findings"]["waived"], 0,
            "{bound_to}: a bundle that binds to nothing here suppressed a finding anyway"
        );
    }
}

/// A waiver item names the candidate tree it was written against, and the run
/// evaluates exactly one tree. An item written for a different one is not a defect
/// and not an error: it is simply not addressed to this evaluation, so it is never
/// selected, and the finding it names stands. The distinction matters because the
/// bundle around it verifies perfectly. Repository, branch, floor, and instant all
/// bind, the controls are trustworthy, and the suppression still does not happen,
/// which is the only thing standing between a waiver issued for last week's tree and
/// a broken link that walks into today's.
#[test]
fn a_waiver_item_written_for_another_tree_is_never_selected() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(Profile::Enforce);
    setup.time = Some(time_input(&fx));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.base_tree, // the tree before this one, not the tree under evaluation
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-08-01T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(
        report["controls"]["waiver_bundle"]["status"], "verified",
        "the bundle binds: this is not a rejected bundle, it is an unselected item"
    );
    assert_eq!(report["exit_code"], 1, "the finding it names still fails");
    assert_eq!(report["result"]["status"], "fail");
    assert_eq!(report["summary"]["findings"]["waived"], 0);

    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(finding["effective_disposition"], "fail");
    assert!(
        finding["waiver"].is_null(),
        "nothing is recorded against a finding no waiver reached"
    );
}
