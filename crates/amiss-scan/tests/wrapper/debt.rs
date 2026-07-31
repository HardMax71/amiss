use amiss_git::Repository;
use amiss_scan::policy::{ConstraintInput, TrustSource};
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::model::{ObjectFormat, Oid};

use crate::support::{
    Fixture, INSTANT, assert_global_location, debt_input, debt_json, fixture, floor_input, payload,
    shell, structural_evidence, time_input,
};

#[test]
fn valid_active_debt_is_tolerated_with_full_provenance() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    setup.constraint = Some(ConstraintInput {
        descriptor: ExecutionConstraintDescriptor::parse(
            br#"{
  "schema": "amiss/scanner-execution-constraint",
  "action_repository": { "host": "git.example.internal", "owner": "platform/security", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#,
        )
        .unwrap(),
        trust_source: TrustSource::OrganizationPolicy,
    });
    let report = payload(&fx, &setup);

    assert_eq!(
        report["result"]["status"], "pass",
        "debt tolerates in enforce"
    );
    assert_eq!(report["exit_code"], 0);
    assert_eq!(report["evaluation"]["evaluation_instant"], INSTANT);
    assert_eq!(report["evaluation"]["trusted_time"], true);
    assert_eq!(report["controls"]["debt_snapshot"]["status"], "verified");
    assert_eq!(
        report["controls"]["trusted_time_source"]["status"],
        "verified"
    );
    assert_eq!(
        report["controls"]["trusted_time_source"]["statement"]["evaluation_instant"],
        INSTANT
    );
    assert_eq!(
        report["controls"]["execution_constraint"]["status"],
        "verified"
    );
    assert_eq!(
        report["controls"]["execution_constraint"]["descriptor"]["selected_platform"],
        "linux-x86_64"
    );
    assert_eq!(
        report["controls"]["execution_constraint"]["descriptor"]["action_repository"]["host"],
        "git.example.internal"
    );
    assert_eq!(
        report["controls"]["execution_constraint"]["descriptor"]["action_repository"]["owner"],
        "platform/security"
    );

    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(finding["configured_disposition"], "fail");
    assert_eq!(finding["effective_disposition"], "warn");
    let trace = finding["policy_trace"].as_array().unwrap();
    let last = trace.last().unwrap();
    assert_eq!(last["source"], "debt-snapshot");
    assert_eq!(last["rule_id"], "debt/acme/legacy-guide-link");
    assert_eq!(last["before"], "fail");
    assert_eq!(last["after"], "warn");
    assert_eq!(finding["debt"]["debt_id"], "acme/legacy-guide-link");
    assert_eq!(finding["debt"]["owner"], "team:docs-platform");
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 1);
}

#[test]
fn an_expired_debt_item_fails_without_application() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-07-10T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(report["result"]["status"], "fail");
    assert_eq!(report["exit_code"], 1);
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 0);
    let kinds: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"debt-expired"));
    let expired = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "debt-expired")
        .unwrap();
    assert_eq!(
        expired["key_input"]["scope"]["rule_id"],
        "debt/acme/legacy-guide-link/expired"
    );
    assert_eq!(
        expired["candidate_fact"]["evidence"]["exception"]["kind"],
        "debt"
    );
    assert_global_location(expired);
    let feedback = &report["feedback"];
    assert_eq!(feedback["status"], "available");
    let item = feedback["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["finding_kinds"] == serde_json::json!(["debt-expired"]))
        .expect("the global defect remains actionable without a repository target");
    assert_eq!(item["action"], "fix");
    assert!(item["target"].is_null());
    assert!(item["annotation"].is_null());
    let structural = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(structural["effective_disposition"], "fail");
    assert_eq!(structural["debt"], serde_json::Value::Null);
}

#[test]
fn a_changed_fact_is_debt_worsened() {
    let fx = fixture("see [gone](missing.md)\n\nsee [gone](missing.md)\n");
    let base_only = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&base_only, true);
    drop(base_only);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
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

    assert_eq!(report["result"]["status"], "fail");
    let worsened = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "debt-worsened")
        .expect("the duplicated occurrence changes the fact digest");
    assert_eq!(
        worsened["key_input"]["scope"]["rule_id"],
        "debt/acme/legacy-guide-link/fact"
    );
    assert_global_location(worsened);
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 0);
}

#[test]
fn a_nonreproducing_adoption_binding_is_fatal() {
    let chain = amiss_fixtures::commit_chain(&[
        ("ancient", &[("README.md", "clean\n")]),
        ("base", &[("README.md", "see [gone](missing.md)\n")]),
        ("candidate", &[("note.md", "[readme](README.md)\n")]),
    ])
    .unwrap();
    let ancient_tree = chain.commits.first().unwrap().tree.clone();
    let fx = Fixture {
        repo: Repository::open(chain.root(), ObjectFormat::Sha1).unwrap(),
        base: Oid::new(ObjectFormat::Sha1, chain.commits.get(1).unwrap().id.clone()).unwrap(),
        candidate: Oid::new(ObjectFormat::Sha1, chain.commits.get(2).unwrap().id.clone()).unwrap(),
        base_tree: chain.commits.get(1).unwrap().tree.clone(),
        candidate_tree: chain.commits.get(2).unwrap().tree.clone(),
        _pair: chain,
    };
    let (finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &ancient_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    let report = payload(&fx, &setup);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["result"]["status"], "incomplete");
    assert_eq!(report["controls"]["status"], "unavailable");
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["control-binding-mismatch"])
    );
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "CONTROL_BINDING_MISMATCH")
    );
}

/// The debt snapshot binds exactly as the waiver bundle does, and for the same
/// reason: it is the control that lets a failing finding be carried instead of
/// fixed, so a snapshot honoured while bound to some other repository, branch, or
/// floor is a way to import tolerance this repository was never granted. It also
/// binds every item's owner to the floor's authorized list, which is the part that
/// stops a snapshot from inventing an owner and accepting debt on their behalf. One
/// arm of this, the adoption tree, had a test. None of the rest did.
#[test]
fn a_debt_snapshot_bound_to_anything_else_verifies_nothing_and_tolerates_nothing() {
    let fx = fixture("see [gone](missing.md)\n");
    let (finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let valid = debt_json(
        &floor_digest,
        &fx.base_tree,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
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
        (
            "an owner the floor never authorized",
            valid.replace(
                r#""owner": "team:docs-platform""#,
                r#""owner": "team:nobody""#,
            ),
        ),
    ];

    for (bound_to, doc) in cases {
        assert_ne!(
            doc, valid,
            "{bound_to}: the fixture did not actually change"
        );
        let mut setup = shell(true);
        setup.time = Some(time_input(&fx, true));
        setup.debt = Some(debt_input(&doc));
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
            report["summary"]["findings"]["debt_tolerated"], 0,
            "{bound_to}: a snapshot that binds to nothing here carried a finding anyway"
        );
    }
}
