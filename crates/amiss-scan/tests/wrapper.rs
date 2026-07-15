#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::staged_index;
use amiss_scan::policy::{
    ConstraintInput, DebtInput, FloorInput, TimeInput, TrustSource, WaiverInput,
};
use amiss_scan::report::{CandidateBlock, candidate_identity_digest};
use amiss_scan::{Effects, Setup, SetupShell, SnapshotIdentity, commit_pair};
use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor, TrustedTimeStatement,
    WaiverBundle,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

const INSTANT: &str = "2026-07-12T10:00:00Z";

fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"test engine"),
    }
}

/// The standing fixture: the base introduces one missing-target link, the
/// candidate keeps it, so the structural finding is pre-existing and the
/// base tree doubles as a reproducible adoption tree.
struct Fixture {
    _dir: TempDir,
    repo: Repository,
    base: Oid,
    candidate: Oid,
    base_tree: String,
    candidate_tree: String,
}

fn fixture(candidate_readme: &str) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "see [gone](missing.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("README.md"), candidate_readme).unwrap();
    fs::write(root.join("note.md"), "[readme](README.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let base = git(root, &["rev-parse", "HEAD~1"]).trim().to_owned();
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let base_tree = git(root, &["rev-parse", "HEAD~1^{tree}"]).trim().to_owned();
    let candidate_tree = git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    Fixture {
        _dir: dir,
        repo,
        base: Oid::new(ObjectFormat::Sha1, base).unwrap(),
        candidate: Oid::new(ObjectFormat::Sha1, candidate).unwrap(),
        base_tree,
        candidate_tree,
    }
}

fn floor_input() -> FloorInput {
    let doc = r#"{
  "schema": "amiss/organization-floor/v1",
  "floor_id": "acme/scanner-floor-2026-07",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [ "explicit-target-missing" ],
  "authorized_debt_owners": [ "team:docs-platform" ],
  "authorized_waiver_issuers": [ "team:release-engineering" ],
  "resource_limits": []
}"#;
    FloorInput {
        floor: OrganizationFloor::parse(doc.as_bytes()).unwrap(),
        trust_source: TrustSource::OrganizationRuleset,
    }
}

fn shell(enforce: bool) -> SetupShell {
    SetupShell {
        engine: engine(),
        enforce,
        repository: Some(amiss_wire::model::RepositoryIdentity {
            host: "github.com".to_owned(),
            owner: "acme".to_owned(),
            name: "docs".to_owned(),
        }),
        candidate_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: None,
        floor: Some(floor_input()),
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}

fn identity(commit: &Oid, tree: &str) -> SnapshotIdentity {
    SnapshotIdentity {
        object_format: "sha1",
        commit_oid: commit.as_str().to_owned(),
        tree_oid: tree.to_owned(),
    }
}

fn time_input(fx: &Fixture, enforce: bool) -> TimeInput {
    let setup = Setup {
        engine: engine(),
        enforce,
        repository: Some(amiss_wire::model::RepositoryIdentity {
            host: "github.com".to_owned(),
            owner: "acme".to_owned(),
            name: "docs".to_owned(),
        }),
        candidate_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: None,
        base: identity(&fx.base, &fx.base_tree),
        candidate: CandidateBlock::Commit(identity(&fx.candidate, &fx.candidate_tree)),
        policy: Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    let digest = candidate_identity_digest(&setup);
    let doc = format!(
        r#"{{
  "schema": "amiss/scanner-trusted-time-statement/v1",
  "controller": "github-actions-required-workflow-clock-v1",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "candidate_identity_digest": "{digest}",
  "provider_run_id": "987654321",
  "provider_run_attempt": 2,
  "evaluation_instant": "{INSTANT}",
  "valid_until": "2026-07-12T10:09:00Z"
}}"#
    );
    TimeInput {
        statement: TrustedTimeStatement::parse(doc.as_bytes()).unwrap(),
        provider_run_id: "987654321".to_owned(),
        provider_run_attempt: 2,
    }
}

/// One clean run whose report supplies the exact key and fact values the
/// engine computes for the pre-existing structural finding.
fn structural_evidence(fx: &Fixture, enforce: bool) -> (String, String, String, String) {
    let built = commit_pair(
        &fx.repo,
        &engine(),
        None,
        &shell(enforce),
        &fx.base,
        &fx.candidate,
    );
    let envelope: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let finding = envelope["payload"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .expect("the fixture produces the structural finding");
    (
        serde_json::to_string(&finding["key_input"]).unwrap(),
        finding["finding_key"].as_str().unwrap().to_owned(),
        serde_json::to_string(&finding["candidate_fact"]).unwrap(),
        finding["candidate_fact_digest"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

#[expect(clippy::too_many_arguments, reason = "test fixture builder")]
fn debt_json(
    floor_digest: &str,
    adoption_tree: &str,
    key_input: &str,
    finding_key: &str,
    fact: &str,
    fact_digest: &str,
    created: &str,
    expires: &str,
) -> String {
    format!(
        r#"{{
  "schema": "amiss/debt-snapshot/v1",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "{floor_digest}",
  "adoption_tree": {{ "object_format": "sha1", "tree_oid": "{adoption_tree}" }},
  "adoption_report_payload_digest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "created_at": "2026-07-03T00:00:00Z",
  "items": [ {{
    "debt_id": "acme/legacy-guide-link",
    "finding_kind": "explicit-target-missing",
    "key_input": {key_input},
    "finding_key": "{finding_key}",
    "accepted_fact": {fact},
    "accepted_fact_digest": "{fact_digest}",
    "owner": "team:docs-platform",
    "reason": "Legacy link scheduled for removal.",
    "created_at": "{created}",
    "expires_at": "{expires}"
  }} ]
}}"#
    )
}

fn debt_input(doc: &str) -> DebtInput {
    DebtInput {
        snapshot: DebtSnapshot::parse(doc.as_bytes())
            .map_err(|defect| format!("{defect:?}"))
            .unwrap(),
        trust_source: TrustSource::ExternalRequiredWorkflow,
    }
}

#[expect(clippy::too_many_arguments, reason = "test fixture builder")]
fn waiver_json(
    floor_digest: &str,
    candidate_tree: &str,
    key_input: &str,
    finding_key: &str,
    fact: &str,
    fact_digest: &str,
    issuer: &str,
    expires: &str,
) -> String {
    format!(
        r#"{{
  "schema": "amiss/waiver-bundle/v1",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "{floor_digest}",
  "created_at": "2026-07-03T00:00:00Z",
  "items": [ {{
    "waiver_id": "acme/release-window",
    "finding_kind": "explicit-target-missing",
    "key_input": {key_input},
    "finding_key": "{finding_key}",
    "authorized_fact": {fact},
    "authorized_fact_digest": "{fact_digest}",
    "candidate_tree": {{ "object_format": "sha1", "tree_oid": "{candidate_tree}" }},
    "owner": "team:docs-platform",
    "issuer": "{issuer}",
    "reason": "Release window exception.",
    "created_at": "2026-07-01T00:00:00Z",
    "not_before": "2026-07-02T00:00:00Z",
    "expires_at": "{expires}",
    "residual_disposition": "warn"
  }} ]
}}"#
    )
}

fn waiver_input(doc: &str) -> WaiverInput {
    WaiverInput {
        bundle: WaiverBundle::parse(doc.as_bytes())
            .map_err(|defect| format!("{defect:?}"))
            .unwrap(),
        trust_source: TrustSource::ExternalRequiredWorkflow,
    }
}

fn payload(fx: &Fixture, setup: &SetupShell) -> serde_json::Value {
    let built = commit_pair(&fx.repo, &engine(), None, setup, &fx.base, &fx.candidate);
    let envelope: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report-v2.schema.json"),
    )
    .unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(defects, Vec::<String>::new(), "schema-clean report");
    let mut value = envelope["payload"].clone();
    value["exit_code"] = serde_json::Value::from(built.exit_code);
    value
}

#[test]
fn valid_active_debt_is_tolerated_with_full_provenance() {
    let fx = fixture("see [gone](missing.md)\n");
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    setup.constraint = Some(ConstraintInput {
        descriptor: ExecutionConstraintDescriptor::parse(
            br#"{
  "schema": "amiss/scanner-execution-constraint/v1",
  "action_repository": { "host": "github.com", "owner": "acme", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap-v1",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#,
        )
        .unwrap(),
        trust_source: TrustSource::OrganizationRuleset,
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&base_only, true);
    drop(base_only);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
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
    assert_eq!(report["summary"]["findings"]["debt_tolerated"], 0);
}

#[test]
fn a_nonreproducing_adoption_binding_is_fatal() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "clean\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "ancient"]);
    let ancient_tree = git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    fs::write(root.join("README.md"), "see [gone](missing.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("note.md"), "[readme](README.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let fx = Fixture {
        repo: Repository::open(root, ObjectFormat::Sha1).unwrap(),
        base: Oid::new(
            ObjectFormat::Sha1,
            git(root, &["rev-parse", "HEAD~1"]).trim().to_owned(),
        )
        .unwrap(),
        candidate: Oid::new(
            ObjectFormat::Sha1,
            git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
        )
        .unwrap(),
        base_tree: git(root, &["rev-parse", "HEAD~1^{tree}"]).trim().to_owned(),
        candidate_tree: git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned(),
        _dir: dir,
    };
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &ancient_tree,
        &key_input,
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

#[test]
fn a_valid_waiver_changes_fail_to_warn() {
    let fx = fixture("see [gone](missing.md)\n");
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &key_input,
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();

    let unauthorized = waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &key_input,
        &finding_key,
        &fact,
        &fact_digest,
        "team:docs-platform",
        "2026-08-01T00:00:00Z",
    );
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.waiver = Some(waiver_input(&unauthorized));
    let report = payload(&fx, &setup);

    assert_eq!(report["result"]["status"], "fail");
    assert_eq!(report["exit_code"], 1);
    assert_eq!(report["summary"]["findings"]["waived"], 0);
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
        &key_input,
        &finding_key,
        &fact,
        &fact_digest,
        "team:release-engineering",
        "2026-07-10T00:00:00Z",
    );
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.waiver = Some(waiver_input(&expired));
    let report = payload(&fx, &setup);
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

#[test]
fn overlapping_valid_exceptions_are_fatal_and_apply_neither() {
    let fx = fixture("see [gone](missing.md)\n");
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
        &finding_key,
        &fact,
        &fact_digest,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    )));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &key_input,
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
fn expiry_bearing_controls_require_a_trusted_instant() {
    let fx = fixture("see [gone](missing.md)\n");
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
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
    let mut setup = shell(false);
    let mut time = time_input(&fx, false);
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.debt = Some(debt_input(&debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let valid = waiver_json(
        &floor_digest,
        &fx.candidate_tree,
        &key_input,
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
        let mut setup = shell(true);
        setup.time = Some(time_input(&fx, true));
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let mut setup = shell(true);
    setup.time = Some(time_input(&fx, true));
    setup.waiver = Some(waiver_input(&waiver_json(
        &floor_digest,
        &fx.base_tree, // the tree before this one, not the tree under evaluation
        &key_input,
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
    let (key_input, finding_key, fact, fact_digest) = structural_evidence(&fx, true);
    let floor_digest = floor_input().floor.digest.to_string();
    let valid = debt_json(
        &floor_digest,
        &fx.base_tree,
        &key_input,
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
