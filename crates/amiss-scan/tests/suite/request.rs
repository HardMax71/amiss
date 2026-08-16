#![expect(
    clippy::expect_used,
    reason = "integration assertions over the external-control request gate"
)]

use amiss_scan::request::controls;
use amiss_wire::digest::hb;
use amiss_wire::json::parse;
use amiss_wire::report::AnalysisErrorCode;
use amiss_wire::requests::{ControlsRequest, RequestTrust, SuppliedControl, SuppliedTime};

const FLOOR: &str = r#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/scanner-floor-2026-07",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}"#;

const TIME: &str = r#"{
  "schema": "amiss/scanner-trusted-time-statement",
  "controller": "external-required-check-clock",
  "repository": { "host": "gitlab.com", "owner": "platform/security", "name": "docs" },
  "ref": "refs/heads/main",
  "candidate_identity_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  "provider": "gitlab-ci",
  "provider_run_id": "pipeline/01J2Z9-7",
  "provider_run_attempt": 2,
  "evaluation_instant": "2026-07-12T10:00:00Z",
  "valid_until": "2026-07-12T10:10:00Z"
}"#;

const CONSTRAINT: &str = r#"{
  "schema": "amiss/scanner-execution-constraint",
  "action_repository": { "host": "github.com", "owner": "acme", "name": "amiss-action" },
  "action_object_format": "sha1",
  "action_commit_oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "action_tree_oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "manifest_path": "release/manifest.json",
  "release_manifest_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "selected_platform": "linux-x86_64",
  "required_status_name": "amiss / documentation assurance",
  "bootstrap_contract": "amiss-action-bootstrap",
  "bootstrap_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333"
}"#;

const fn empty() -> ControlsRequest {
    ControlsRequest {
        organization_floor: None,
        debt_snapshot: None,
        waiver_bundle: None,
        trusted_time: None,
        execution_constraint: None,
    }
}

fn supplied(doc: &str, expected: amiss_wire::digest::Digest) -> SuppliedControl {
    SuppliedControl {
        value: parse(doc.as_bytes()).expect("the fixture is JSON"),
        expected_digest: expected,
        trust_source: RequestTrust::OrganizationPolicy,
    }
}

#[test]
fn a_verified_floor_lands_typed() {
    let floor =
        amiss_wire::controls::OrganizationFloor::parse(FLOOR.as_bytes()).expect("fixture parses");
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, floor.digest()));
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.floor.expect("the floor lands typed");
    assert_eq!(landed.floor.digest(), floor.digest());
    assert!(inputs.time.is_none() && inputs.debt.is_none());
}

#[test]
fn a_wrong_floor_digest_is_refused() {
    let mut request = empty();
    request.organization_floor = Some(supplied(FLOOR, hb("test/other", b"not the floor")));
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn a_verified_time_statement_lands_with_its_run_context() {
    let statement =
        amiss_wire::controls::TrustedTimeStatement::parse(TIME.as_bytes()).expect("fixture parses");
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: parse(TIME.as_bytes()).expect("the fixture is JSON"),
        expected_digest: statement.digest(),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.time.expect("the statement lands typed");
    assert_eq!(landed.statement.digest(), statement.digest());
    assert_eq!(landed.provider, "gitlab-ci");
    assert_eq!(landed.provider_run_id, "pipeline/01J2Z9-7");
    assert_eq!(landed.provider_run_attempt, 2);
}

#[test]
fn a_wrong_time_digest_is_refused() {
    let mut request = empty();
    request.trusted_time = Some(SuppliedTime {
        value: parse(TIME.as_bytes()).expect("the fixture is JSON"),
        expected_digest: hb("test/other", b"not the statement"),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
    });
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}

#[test]
fn a_verified_constraint_lands_through_the_shared_gate() {
    let descriptor =
        amiss_wire::controls::ExecutionConstraintDescriptor::parse(CONSTRAINT.as_bytes())
            .expect("fixture parses");
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, descriptor.digest()));
    let inputs = controls(&request).expect("a matching digest passes the gate");
    let landed = inputs.constraint.expect("the descriptor lands typed");
    assert_eq!(landed.descriptor.digest(), descriptor.digest());
}

#[test]
fn a_wrong_constraint_digest_is_refused() {
    let mut request = empty();
    request.execution_constraint = Some(supplied(CONSTRAINT, hb("test/other", b"not the plan")));
    let error = controls(&request).expect_err("a foreign digest never passes");
    assert_eq!(error.code, AnalysisErrorCode::DigestMismatch);
}
