#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::policy::{DebtInput, FloorInput, TimeInput, WaiverInput};
use amiss_scan::report::{CandidateBlock, candidate_identity_digest};
use amiss_scan::{Effects, Setup, SetupShell, SnapshotIdentity, commit_pair};
use amiss_wire::controls::{
    DebtSnapshot, OrganizationFloor, Profile, WaiverBundle, parse_trusted_time,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use amiss_wire::requests::RequestTrust;

pub(crate) const INSTANT: &str = "2026-07-12T10:00:00Z";

pub(crate) fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

/// The standing fixture: the base introduces one missing-target link, the
/// candidate keeps it, so the structural finding is pre-existing and the
/// base tree doubles as a reproducible adoption tree.
pub(crate) struct Fixture {
    pub(crate) _pair: amiss_fixtures::CommitChain,
    pub(crate) repo: Repository,
    pub(crate) base: Oid,
    pub(crate) candidate: Oid,
    pub(crate) base_tree: String,
    pub(crate) candidate_tree: String,
}

pub(crate) fn fixture(candidate_readme: &str) -> Fixture {
    let chain = amiss_fixtures::commit_chain(&[
        ("base", &[("README.md", "see [gone](missing.md)\n")]),
        (
            "candidate",
            &[
                ("README.md", candidate_readme),
                ("note.md", "[readme](README.md)\n"),
            ],
        ),
    ])
    .unwrap();
    Fixture {
        repo: Repository::open(chain.root(), ObjectFormat::Sha1).unwrap(),
        base: Oid::new(
            ObjectFormat::Sha1,
            chain.commits.first().unwrap().id.clone(),
        )
        .unwrap(),
        candidate: Oid::new(ObjectFormat::Sha1, chain.commits.get(1).unwrap().id.clone()).unwrap(),
        base_tree: chain.commits.first().unwrap().tree.clone(),
        candidate_tree: chain.commits.get(1).unwrap().tree.clone(),
        _pair: chain,
    }
}

pub(crate) fn floor_input() -> FloorInput {
    let doc = r#"{
  "schema": "amiss/organization-floor",
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
        trust_source: RequestTrust::OrganizationPolicy,
    }
}

pub(crate) fn shell(profile: Profile) -> SetupShell {
    SetupShell {
        engine: engine(),
        profile,
        repository: Some(
            amiss_wire::model::RepositoryIdentity::github("acme".to_owned(), "docs".to_owned())
                .unwrap(),
        ),
        forge: Some(amiss_wire::model::ForgeDialect::Github),
        candidate_ref: Some("refs/heads/main".to_owned()),
        target_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: None,
        floor: Some(floor_input()),
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}

pub(crate) fn identity(commit: &Oid, tree: &str) -> SnapshotIdentity {
    SnapshotIdentity {
        object_format: "sha1",
        commit_oid: commit.as_str().to_owned(),
        tree_oid: tree.to_owned(),
    }
}

pub(crate) fn time_input(fx: &Fixture) -> TimeInput {
    let setup = Setup {
        engine: engine(),
        profile: Profile::Observe,
        repository: Some(
            amiss_wire::model::RepositoryIdentity::github("acme".to_owned(), "docs".to_owned())
                .unwrap(),
        ),
        forge: Some(amiss_wire::model::ForgeDialect::Github),
        candidate_ref: Some("refs/heads/main".to_owned()),
        target_ref: Some("refs/heads/main".to_owned()),
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
  "schema": "amiss/scanner-trusted-time-statement",
  "controller": "external-required-check-clock",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "candidate_identity_digest": "{digest}",
  "provider": "gitlab-ci",
  "provider_run_id": "pipeline/987654321",
  "provider_run_attempt": 2,
  "evaluation_instant": "{INSTANT}",
  "valid_until": "2026-07-12T10:09:00Z"
}}"#
    );
    let statement = parse_trusted_time(doc.as_bytes()).unwrap();
    TimeInput {
        statement,
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/987654321".to_owned(),
        provider_run_attempt: 2,
    }
}

/// One clean run whose report supplies the exact key and fact values the
/// engine computes for the pre-existing structural finding.
pub(crate) fn structural_evidence(fx: &Fixture) -> (String, String, String) {
    let built = commit_pair(
        &fx.repo,
        &engine(),
        None,
        &shell(Profile::Enforce),
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
        finding["finding_key"].as_str().unwrap().to_owned(),
        serde_json::to_string(&finding["candidate_fact"]).unwrap(),
        finding["candidate_fact_digest"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

pub(crate) fn debt_json(
    floor_digest: &str,
    adoption_tree: &str,
    finding_key: &str,
    fact: &str,
    fact_digest: &str,
    created: &str,
    expires: &str,
) -> String {
    format!(
        r#"{{
  "schema": "amiss/debt-snapshot",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "{floor_digest}",
  "adoption_tree": {{ "object_format": "sha1", "tree_oid": "{adoption_tree}" }},
  "adoption_report_payload_digest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
  "created_at": "2026-07-03T00:00:00Z",
  "items": [ {{
    "debt_id": "acme/legacy-guide-link",
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

pub(crate) fn debt_input(doc: &str) -> DebtInput {
    DebtInput {
        snapshot: DebtSnapshot::parse(doc.as_bytes())
            .map_err(|defect| format!("{defect:?}"))
            .unwrap(),
        trust_source: RequestTrust::ExternalRequiredCheck,
    }
}

pub(crate) fn waiver_json(
    floor_digest: &str,
    candidate_tree: &str,
    finding_key: &str,
    fact: &str,
    fact_digest: &str,
    issuer: &str,
    expires: &str,
) -> String {
    format!(
        r#"{{
  "schema": "amiss/waiver-bundle",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "organization_floor_digest": "{floor_digest}",
  "created_at": "2026-07-03T00:00:00Z",
  "items": [ {{
    "waiver_id": "acme/release-window",
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

pub(crate) fn waiver_input(doc: &str) -> WaiverInput {
    WaiverInput {
        bundle: WaiverBundle::parse(doc.as_bytes())
            .map_err(|defect| format!("{defect:?}"))
            .unwrap(),
        trust_source: RequestTrust::ExternalRequiredCheck,
    }
}

pub(crate) fn payload(fx: &Fixture, setup: &SetupShell) -> serde_json::Value {
    let built = commit_pair(&fx.repo, &engine(), None, setup, &fx.base, &fx.candidate);
    let envelope: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
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

pub(crate) fn assert_global_location(finding: &serde_json::Value) {
    assert_eq!(
        finding["location"],
        serde_json::json!({"side": "global", "path": null, "span": null})
    );
}
