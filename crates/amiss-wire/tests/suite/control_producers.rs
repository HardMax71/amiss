#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid typed fixtures and inspect expected failures"
)]

use std::fs;
use std::path::Path;

use amiss_wire::controls::{
    ActionBootstrapContract, ConstraintPlatform, ExecutionConstraintDescriptor,
    ExecutionConstraintSchema, TrustedTimeController, TrustedTimeSchema, TrustedTimeStatement,
    canonical_execution_constraint, canonical_trusted_time, parse_execution_constraint,
    parse_trusted_time, valid_required_status_name,
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

fn trusted_time_statement() -> TrustedTimeStatement {
    TrustedTimeStatement {
        schema: TrustedTimeSchema::Current,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
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

fn execution_constraint() -> ExecutionConstraintDescriptor {
    ExecutionConstraintDescriptor {
        schema: ExecutionConstraintSchema::Current,
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
        bootstrap_contract: ActionBootstrapContract::Current,
        bootstrap_digest: Digest::from_wire(DIGEST_C).unwrap(),
    }
}

#[test]
fn trusted_time_model_and_writer_share_the_parser_contract() {
    let statement = trusted_time_statement();
    let (bytes, _) = canonical_trusted_time(&statement).unwrap();

    assert_eq!(parse_trusted_time(&bytes).unwrap(), statement);
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    for attempt in [0, 9_007_199_254_740_992, u64::MAX] {
        let mut invalid = trusted_time_statement();
        invalid.provider_run_attempt = attempt;
        let error = canonical_trusted_time(&invalid).unwrap_err();
        assert_eq!(error.path, "$.provider_run_attempt");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
    let mut invalid = trusted_time_statement();
    invalid.valid_until = invalid.evaluation_instant.clone();
    let error = canonical_trusted_time(&invalid).unwrap_err();
    assert_eq!(error.path, "$.valid_until");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn execution_constraint_model_and_writer_share_the_parser_contract() {
    let descriptor = execution_constraint();
    let (bytes, _) = canonical_execution_constraint(&descriptor).unwrap();

    assert_eq!(parse_execution_constraint(&bytes).unwrap(), descriptor);
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    let mut invalid = execution_constraint();
    invalid.action_object_format = ObjectFormat::Sha256;
    let error = canonical_execution_constraint(&invalid).unwrap_err();
    assert_eq!(error.path, "$.action_commit_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
    let mut invalid = execution_constraint();
    invalid.required_status_name = " trailing ".to_owned();
    let error = canonical_execution_constraint(&invalid).unwrap_err();
    assert_eq!(error.path, "$.required_status_name");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn producer_writers_preserve_the_validated_digests() {
    let statement = trusted_time_statement();
    let (statement_bytes, statement_digest) = canonical_trusted_time(&statement).unwrap();
    let parsed = parse_trusted_time(&statement_bytes).unwrap();
    assert_eq!(canonical_trusted_time(&parsed).unwrap().1, statement_digest);

    let descriptor = execution_constraint();
    let (descriptor_bytes, descriptor_digest) =
        canonical_execution_constraint(&descriptor).unwrap();
    let parsed = parse_execution_constraint(&descriptor_bytes).unwrap();
    assert_eq!(
        canonical_execution_constraint(&parsed).unwrap().1,
        descriptor_digest
    );
}

#[test]
fn required_status_names_share_one_public_grammar() {
    for valid in [
        "a",
        "amiss / documentation assurance",
        "docs.check_1",
        "amiss:policy",
    ] {
        assert!(valid_required_status_name(valid), "{valid}");
    }
    for invalid in ["", " amiss", "amiss ", &"a".repeat(161)] {
        assert!(!valid_required_status_name(invalid), "{invalid}");
    }
}

#[test]
fn producer_writers_preserve_the_published_contract_examples() {
    let trusted_time = example("scanner-trusted-time-statement.json");
    let statement = parse_trusted_time(&trusted_time).unwrap();
    assert_eq!(
        canonical_trusted_time(&statement).unwrap().0,
        json::canonical(&json::parse(&trusted_time).unwrap())
    );

    let execution_constraint = example("scanner-execution-constraint.json");
    let descriptor = parse_execution_constraint(&execution_constraint).unwrap();
    assert_eq!(
        canonical_execution_constraint(&descriptor).unwrap().0,
        json::canonical(&json::parse(&execution_constraint).unwrap())
    );
}
