#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid typed fixtures and inspect expected failures"
)]

use std::fs;
use std::path::Path;

use amiss_wire::controls::{
    ConstraintPlatform, ExecutionConstraintDescriptor, ExecutionConstraintInput, TrustedTimeInput,
    TrustedTimeStatement, valid_required_status_name,
};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::Digest;
use amiss_wire::json;
use amiss_wire::model::{
    BranchRef, ObjectFormat, Oid, RepoPathText, RepositoryIdentity, UtcInstant,
};

const DIGEST_A: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const DIGEST_B: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const DIGEST_C: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";

fn example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .unwrap()
}

fn repository() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "gitlab.com".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    )
    .unwrap()
}

fn trusted_time_input() -> TrustedTimeInput {
    TrustedTimeInput {
        repository: repository(),
        ref_name: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        candidate_identity_digest: Digest::from_wire(DIGEST_A).unwrap(),
        provider: "gitlab-ci".to_owned(),
        provider_run_id: "pipeline/01J2Z9-7".to_owned(),
        provider_run_attempt: 2,
        evaluation_instant: UtcInstant::new("2026-07-12T10:00:00Z".to_owned()).unwrap(),
        valid_until: UtcInstant::new("2026-07-12T10:10:00Z".to_owned()).unwrap(),
    }
}

fn execution_constraint_input() -> ExecutionConstraintInput {
    ExecutionConstraintInput {
        action_repository: RepositoryIdentity::github("acme".to_owned(), "amiss-action".to_owned())
            .unwrap(),
        action_object_format: ObjectFormat::Sha1,
        action_commit_oid: Oid::new(
            ObjectFormat::Sha1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        )
        .unwrap(),
        action_tree_oid: Oid::new(
            ObjectFormat::Sha1,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        )
        .unwrap(),
        manifest_path: RepoPathText::new("release/manifest.json".to_owned()).unwrap(),
        release_manifest_digest: Digest::from_wire(DIGEST_B).unwrap(),
        selected_platform: ConstraintPlatform::LinuxX8664,
        required_status_name: "amiss / documentation assurance".to_owned(),
        bootstrap_digest: Digest::from_wire(DIGEST_C).unwrap(),
    }
}

#[test]
fn trusted_time_constructor_and_writer_share_the_parser_contract() {
    let statement = TrustedTimeStatement::new(trusted_time_input()).unwrap();
    let bytes = statement.canonical_bytes().unwrap();

    assert_eq!(TrustedTimeStatement::parse(&bytes).unwrap(), statement);
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    for attempt in [0, 9_007_199_254_740_992, u64::MAX] {
        let mut invalid = trusted_time_input();
        invalid.provider_run_attempt = attempt;
        let error = TrustedTimeStatement::new(invalid).unwrap_err();
        assert_eq!(error.path, "$.provider_run_attempt");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
}

#[test]
fn trusted_time_writer_rejects_a_stale_digest() {
    let mut statement = TrustedTimeStatement::new(trusted_time_input()).unwrap();
    statement.provider_run_id = "pipeline/01J2Z9-8".to_owned();

    let error = statement.canonical_bytes().unwrap_err();
    assert_eq!(error.path, "$.digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
}

#[test]
fn execution_constraint_constructor_and_writer_share_the_parser_contract() {
    let descriptor = ExecutionConstraintDescriptor::new(execution_constraint_input()).unwrap();
    let bytes = descriptor.canonical_bytes().unwrap();

    assert_eq!(
        ExecutionConstraintDescriptor::parse(&bytes).unwrap(),
        descriptor
    );
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    let mut invalid = execution_constraint_input();
    invalid.action_object_format = ObjectFormat::Sha256;
    let error = ExecutionConstraintDescriptor::new(invalid).unwrap_err();
    assert_eq!(error.path, "$.action_commit_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn execution_constraint_writer_rejects_a_stale_digest() {
    let mut descriptor = ExecutionConstraintDescriptor::new(execution_constraint_input()).unwrap();
    descriptor.required_status_name = "amiss / docs".to_owned();

    let error = descriptor.canonical_bytes().unwrap_err();
    assert_eq!(error.path, "$.digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);
}

#[test]
fn required_status_names_share_one_public_grammar() {
    for valid in ["a", "amiss / documentation assurance", "docs.check_1"] {
        assert!(valid_required_status_name(valid), "{valid}");
    }
    for invalid in ["", " amiss", "amiss ", "amiss:docs", &"a".repeat(161)] {
        assert!(!valid_required_status_name(invalid), "{invalid}");
    }
}

#[test]
fn producer_writers_preserve_the_published_contract_examples() {
    let trusted_time = example("scanner-trusted-time-statement.json");
    let statement = TrustedTimeStatement::parse(&trusted_time).unwrap();
    assert_eq!(
        statement.canonical_bytes().unwrap(),
        json::canonical(&json::parse(&trusted_time).unwrap())
    );

    let execution_constraint = example("scanner-execution-constraint.json");
    let descriptor = ExecutionConstraintDescriptor::parse(&execution_constraint).unwrap();
    assert_eq!(
        descriptor.canonical_bytes().unwrap(),
        json::canonical(&json::parse(&execution_constraint).unwrap())
    );
}
