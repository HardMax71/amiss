use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor, TrustedTimeStatement,
    WaiverBundle,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::json;

use crate::support::{
    FLOOR, TIME_STATEMENT, computed_digests, debt_item, debt_snapshot, waiver_bundle, waiver_item,
};

#[test]
fn controls_accept_open_forge_identities() {
    let floor = OrganizationFloor::parse(FLOOR).unwrap();
    assert_eq!(floor.schema(), "amiss/organization-floor");
    assert_eq!(floor.repository().host(), "gitlab.com");
    assert_eq!(floor.repository().owner(), "platform/security");

    let (key, fact) = computed_digests();
    let item = debt_item(
        "debt/readme",
        &key,
        &fact,
        "2026-07-01T00:00:00Z",
        "2026-08-01T00:00:00Z",
    );
    let debt = debt_snapshot("2026-07-02T00:00:00Z", &[item])
        .replace("\"host\": \"github.com\"", "\"host\": \"gitlab.com\"")
        .replace("\"owner\": \"acme\"", "\"owner\": \"platform/security\"");
    let debt_value = json::parse(debt.as_bytes()).unwrap();
    let debt = DebtSnapshot::parse(debt.as_bytes()).unwrap();
    assert_eq!(debt.schema(), "amiss/debt-snapshot");
    assert_eq!(debt.repository().owner(), "platform/security");
    assert_eq!(debt.digest(), hj("amiss/debt-snapshot", &debt_value));

    let item = waiver_item("waiver/one", &key, &fact, "team:release-engineering");
    let waiver = waiver_bundle(&[item])
        .replace("\"host\": \"github.com\"", "\"host\": \"gitlab.com\"")
        .replace("\"owner\": \"acme\"", "\"owner\": \"platform/security\"");
    let waiver_value = json::parse(waiver.as_bytes()).unwrap();
    let waiver = WaiverBundle::parse(waiver.as_bytes()).unwrap();
    assert_eq!(waiver.schema(), "amiss/waiver-bundle");
    assert_eq!(waiver.repository().owner(), "platform/security");
    assert_eq!(waiver.digest(), hj("amiss/waiver-bundle", &waiver_value));

    let time = TrustedTimeStatement::parse(TIME_STATEMENT.as_bytes()).unwrap();
    assert_eq!(time.schema(), "amiss/scanner-trusted-time-statement");
    assert_eq!(time.controller(), "external-required-check-clock");
    assert_eq!(time.repository().owner(), "platform/security");
    assert_eq!(time.provider(), "gitlab-ci");
    assert_eq!(time.provider_run_id(), "pipeline/01J2Z9-7");
}

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

#[test]
fn parses_an_execution_constraint_descriptor() {
    let descriptor = ExecutionConstraintDescriptor::parse(CONSTRAINT.as_bytes()).unwrap();
    assert_eq!(descriptor.selected_platform().as_ref(), "linux-x86_64");
    assert_eq!(
        descriptor.required_status_name(),
        "amiss / documentation assurance"
    );

    let open_repository = CONSTRAINT.replace(
        "\"host\": \"github.com\", \"owner\": \"acme\"",
        "\"host\": \"git.example.internal\", \"owner\": \"platform/security\"",
    );
    let descriptor = ExecutionConstraintDescriptor::parse(open_repository.as_bytes()).unwrap();
    assert_eq!(
        descriptor.action_repository().host(),
        "git.example.internal"
    );
    assert_eq!(descriptor.action_repository().owner(), "platform/security");

    let slash_host = CONSTRAINT.replace("github.com", "git.example/internal");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(slash_host.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let malformed_owner =
        CONSTRAINT.replace("\"owner\": \"acme\"", "\"owner\": \"platform//security\"");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(malformed_owner.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );

    let trailing_space = CONSTRAINT.replace("assurance\"", "assurance \"");
    assert_eq!(
        ExecutionConstraintDescriptor::parse(trailing_space.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    let short_oid = CONSTRAINT.replace(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
    assert_eq!(
        ExecutionConstraintDescriptor::parse(short_oid.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
}
