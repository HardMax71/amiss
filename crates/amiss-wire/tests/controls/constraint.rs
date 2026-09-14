use amiss_wire::controls::ConstraintPlatform;
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::controls::OrganizationFloor;
use amiss_wire::controls::TrustedTimeStatement;
use amiss_wire::controls::{
    ActionBootstrapContract, DebtSnapshot, DebtSnapshotSchema, ExecutionConstraintSchema,
    OrganizationFloorSchema, TrustedTimeController, TrustedTimeSchema, WaiverBundle,
    WaiverBundleSchema,
};
use amiss_wire::de::Document as _;
use amiss_wire::de::ErrorKind;
use sha2::Digest as _;

use super::support::{DEBT, FLOOR, TIME_STATEMENT, WAIVER};

#[test]
fn controls_accept_open_forge_identities() {
    let floor = OrganizationFloor::parse(FLOOR).unwrap();
    assert_eq!(floor.schema, OrganizationFloorSchema::Current);
    assert_eq!(floor.repository.host(), "gitlab.com");
    assert_eq!(floor.repository.owner(), "platform/security");

    let mut debt: DebtSnapshot = serde_json::from_slice(DEBT).unwrap();
    debt.repository = floor.repository.clone();
    assert_eq!(
        DebtSnapshot::parse(&serde_json::to_vec(&debt).unwrap()).unwrap(),
        debt
    );
    assert_eq!(debt.schema, DebtSnapshotSchema::Current);
    assert_eq!(debt.repository.owner(), "platform/security");

    let mut waiver: WaiverBundle = serde_json::from_slice(WAIVER).unwrap();
    waiver.repository = floor.repository;
    assert_eq!(
        WaiverBundle::parse(&serde_json::to_vec(&waiver).unwrap()).unwrap(),
        waiver
    );
    assert_eq!(waiver.schema, WaiverBundleSchema::Current);
    assert_eq!(waiver.repository.owner(), "platform/security");

    let time = TrustedTimeStatement::parse(TIME_STATEMENT.as_bytes()).unwrap();
    assert_eq!(time.schema, TrustedTimeSchema::Current);
    assert_eq!(
        time.controller,
        TrustedTimeController::ExternalRequiredCheckClock
    );
    assert_eq!(time.repository.owner(), "platform/security");
    assert_eq!(time.provider, "gitlab-ci");
    assert_eq!(time.provider_run_id, "pipeline/01J2Z9-7");
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
    assert_eq!(descriptor.schema, ExecutionConstraintSchema::Current);
    assert_eq!(
        descriptor.bootstrap_contract,
        ActionBootstrapContract::Current
    );
    assert_eq!(descriptor.selected_platform.as_ref(), "linux-x86_64");
    assert_eq!(
        descriptor.required_status_name.as_str(),
        "amiss / documentation assurance"
    );
    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-execution-constraint")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&descriptor).unwrap())
                .finalize()
                .0
        ),
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-execution-constraint")
                .chain_update([0_u8])
                .chain_update(
                    serde_json_canonicalizer::to_vec(
                        &serde_json::from_slice::<serde_json::Value>(CONSTRAINT.as_bytes())
                            .unwrap()
                    )
                    .unwrap()
                )
                .finalize()
                .0
        )
    );

    let open_repository = CONSTRAINT.replace(
        "\"host\": \"github.com\", \"owner\": \"acme\"",
        "\"host\": \"git.example.internal\", \"owner\": \"platform/security\"",
    );
    let descriptor = ExecutionConstraintDescriptor::parse(open_repository.as_bytes()).unwrap();
    assert_eq!(descriptor.action_repository.host(), "git.example.internal");
    assert_eq!(descriptor.action_repository.owner(), "platform/security");

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

#[test]
fn platform_artifact_names_follow_the_platform_spelling() {
    for platform in [
        ConstraintPlatform::LinuxX8664,
        ConstraintPlatform::LinuxAarch64,
        ConstraintPlatform::MacosX8664,
        ConstraintPlatform::MacosAarch64,
        ConstraintPlatform::WindowsX8664,
        ConstraintPlatform::WindowsAarch64,
    ] {
        assert_eq!(
            platform.artifact_name().as_str(),
            format!("amiss-{}", platform.as_ref())
        );
    }
}
