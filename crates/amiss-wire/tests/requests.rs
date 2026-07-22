#![expect(
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_wire::controls::{OrganizationFloor, Profile, TrustedTimeStatement};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hj;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use amiss_wire::requests::{
    CANDIDATE_IDENTITY_DOMAIN, ControlsRequest, EvaluationRequest, REPOSITORY_HANDLE_ORDINAL,
    RequestMode, RequestStreams, RequestTrust, SnapshotRequest, commit_candidate_identity_digest,
};

fn request_example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .expect("the specification ships this request example")
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).expect("the test oid matches SHA-1")
}

/// The examples are executable contract fixtures, not illustrations copied out
/// of the schemas. A field or grammar change therefore has to update the parser,
/// schema, and published example together.
#[test]
fn the_request_examples_parse_to_what_they_say() {
    let evaluation =
        EvaluationRequest::parse(&request_example("scanner-evaluation-request.json")).unwrap();
    assert_eq!(evaluation.profile, Profile::Enforce);
    assert_eq!(evaluation.mode, RequestMode::CommitPair);
    assert_eq!(evaluation.object_format, ObjectFormat::Sha1);
    let repository = evaluation
        .repository
        .expect("the example names a repository");
    assert_eq!(repository.host, "gitlab.example.internal");
    assert_eq!(repository.owner, "platform/security");
    assert_eq!(repository.name, "docs");
    assert_eq!(evaluation.forge, Some(ForgeDialect::Gitlab));
    assert_eq!(
        evaluation.candidate_ref.as_ref().map(BranchRef::as_str),
        Some("refs/heads/amiss-controller")
    );
    assert_eq!(
        evaluation.target_ref.as_ref().map(BranchRef::as_str),
        Some("refs/heads/main")
    );
    assert_eq!(
        evaluation.base_commit.as_str(),
        "8d7f2c31a09b64e5dd10fcab7e93245160c8ba72"
    );
    assert_eq!(
        evaluation.candidate_commit.as_ref().map(Oid::as_str),
        Some("3e19afc65b2704d8ce8b1f09a4de6273550d914b"),
        "a commit-pair run names both sides"
    );

    let snapshot =
        SnapshotRequest::parse(&request_example("scanner-snapshot-request.json")).unwrap();
    assert_eq!(snapshot.materialization, RequestMode::CommitPair);

    let controls =
        ControlsRequest::parse(&request_example("scanner-controls-request.json")).unwrap();
    let floor = controls
        .organization_floor
        .expect("the example supplies one");
    assert_eq!(floor.trust_source, RequestTrust::OrganizationPolicy);
    let parsed_floor = OrganizationFloor::parse(&amiss_wire::json::canonical(&floor.value))
        .expect("the embedded organization floor is valid");
    assert_eq!(
        floor.expected_digest, parsed_floor.digest,
        "the request carries the floor's independently reproducible semantic digest"
    );
    let time = controls.trusted_time.expect("and a trusted instant");
    assert_eq!(time.provider, "gitlab");
    assert_eq!(time.provider_run_id, "pipeline/987654321:job-42");
    assert_eq!(time.provider_run_attempt, 2);
    let parsed_time = TrustedTimeStatement::parse(&amiss_wire::json::canonical(&time.value))
        .expect("the embedded trusted-time statement is valid");
    assert_eq!(
        time.expected_digest, parsed_time.digest,
        "the request carries the statement's independently reproducible semantic digest"
    );
    assert!(
        controls.debt_snapshot.is_none()
            && controls.waiver_bundle.is_none()
            && controls.execution_constraint.is_none(),
        "an absent control is absent, never a default"
    );
}

#[test]
fn commit_identity_construction_matches_the_published_preimage() {
    let mut evaluation =
        EvaluationRequest::commit_pair(Profile::Enforce, ObjectFormat::Sha1, oid('1'), oid('3'));
    evaluation.repository = amiss_wire::model::RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    );
    evaluation.forge = Some(ForgeDialect::Gitlab);
    evaluation.candidate_ref = BranchRef::new("refs/heads/amiss-controller".to_owned());
    evaluation.target_ref = BranchRef::new("refs/heads/main".to_owned());
    evaluation.default_branch_ref = BranchRef::new("refs/heads/main".to_owned());

    let published = amiss_wire::json::parse(&request_example("candidate-identity.json"))
        .expect("the candidate identity example is strict JSON");
    assert_eq!(
        commit_candidate_identity_digest(&evaluation, &oid('2'), &oid('4')),
        Some(hj(CANDIDATE_IDENTITY_DOMAIN, &published))
    );

    let index = EvaluationRequest::index(Profile::Enforce, ObjectFormat::Sha1, oid('1'));
    assert_eq!(
        commit_candidate_identity_digest(&index, &oid('2'), &oid('4')),
        None,
        "an index request cannot be relabeled as a commit-pair identity"
    );
}

#[test]
fn wrong_or_legacy_request_contracts_are_not_silent_aliases() {
    let evaluation = String::from_utf8(request_example("scanner-evaluation-request.json")).unwrap();
    let wrong_schema = evaluation.replace(
        "amiss/scanner-evaluation-request",
        "amiss/not-the-scanner-evaluation-request",
    );
    assert!(
        EvaluationRequest::parse(wrong_schema.as_bytes()).is_err(),
        "the rolling contract has one exact unversioned identity"
    );

    let controls = String::from_utf8(request_example("scanner-controls-request.json")).unwrap();
    let legacy_authority = controls.replace("organization-policy", "repository-ruleset");
    assert!(
        ControlsRequest::parse(legacy_authority.as_bytes()).is_err(),
        "only provider-neutral authority roles belong to the current contract"
    );

    let wrong_schema = controls.replace(
        "amiss/scanner-controls-request",
        "amiss/not-the-scanner-controls-request",
    );
    assert!(
        ControlsRequest::parse(wrong_schema.as_bytes()).is_err(),
        "other schema strings do not select hidden parsing modes"
    );

    let snapshot = String::from_utf8(request_example("scanner-snapshot-request.json")).unwrap();
    let wrong_schema = snapshot.replace(
        "amiss/scanner-snapshot-request",
        "amiss/not-the-scanner-snapshot-request",
    );
    assert!(
        SnapshotRequest::parse(wrong_schema.as_bytes()).is_err(),
        "snapshot requests use the same rolling identity rule"
    );
}

#[test]
fn the_forge_and_provider_run_are_closed_and_coherent() {
    let evaluation = String::from_utf8(request_example("scanner-evaluation-request.json")).unwrap();
    let no_repository = evaluation.replace(
        r#""repository": {
    "host": "gitlab.example.internal",
    "owner": "platform/security",
    "name": "docs"
  }"#,
        r#""repository": null"#,
    );
    assert!(
        EvaluationRequest::parse(no_repository.as_bytes()).is_err(),
        "an identity group cannot retain refs after losing its repository"
    );

    let no_target = evaluation.replace(
        r#""target_ref": "refs/heads/main""#,
        r#""target_ref": null"#,
    );
    assert!(
        EvaluationRequest::parse(no_target.as_bytes()).is_err(),
        "the protected target is mandatory whenever an identity is present"
    );

    let github_nested = evaluation.replace(r#""forge": "gitlab""#, r#""forge": "github""#);
    assert!(
        EvaluationRequest::parse(github_nested.as_bytes()).is_err(),
        "GitHub and Gitea dialects never reinterpret a nested GitLab owner"
    );

    let controls = String::from_utf8(request_example("scanner-controls-request.json")).unwrap();
    let edge_punctuation =
        controls.replace("pipeline/987654321:job-42", "/pipeline/987654321:job-42");
    assert!(
        ControlsRequest::parse(edge_punctuation.as_bytes()).is_err(),
        "the opaque run ID still has canonical alphanumeric edges"
    );

    let uppercase_provider = controls.replace(r#""provider": "gitlab""#, r#""provider": "GitLab""#);
    assert!(
        ControlsRequest::parse(uppercase_provider.as_bytes()).is_err(),
        "the provider ID is canonical lowercase"
    );
}

#[test]
fn request_writers_are_canonical_and_the_sealed_frame_is_exact() {
    let evaluation =
        EvaluationRequest::parse(&request_example("scanner-evaluation-request.json")).unwrap();
    let snapshot =
        SnapshotRequest::parse(&request_example("scanner-snapshot-request.json")).unwrap();
    let controls =
        ControlsRequest::parse(&request_example("scanner-controls-request.json")).unwrap();
    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: snapshot.canonical_bytes().unwrap(),
        controls: controls.canonical_bytes().unwrap(),
    };
    for bytes in [&streams.evaluation, &streams.snapshot, &streams.controls] {
        assert_eq!(
            amiss_wire::json::canonical(&amiss_wire::json::parse(bytes).unwrap()),
            *bytes
        );
    }

    let mut frame = Vec::new();
    streams.write_to(&mut frame).unwrap();
    assert_eq!(
        RequestStreams::read_from(&mut frame.as_slice()).unwrap(),
        streams
    );

    frame.push(0);
    assert!(
        RequestStreams::read_from(&mut frame.as_slice()).is_err(),
        "a fourth stream cannot hide after the closed frame"
    );

    for attempt in [0, 9_007_199_254_740_992, u64::MAX] {
        let mut invalid = controls.clone();
        invalid
            .trusted_time
            .as_mut()
            .expect("the request example carries trusted time")
            .provider_run_attempt = attempt;
        let error = invalid.canonical_bytes().unwrap_err();
        assert_eq!(error.path, "$.trusted_time.provider_run_attempt");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
}

/// The candidate commit is null exactly when the mode is `index`. Both ways of
/// getting that wrong describe a run that cannot exist: a commit pair with one
/// side, or a staged index that also names a candidate commit. Neither is a
/// request the engine could act on, and neither parses.
#[test]
fn the_evaluation_request_binds_the_candidate_to_the_mode() {
    let example = String::from_utf8(request_example("scanner-evaluation-request.json")).unwrap();

    let index_with_candidate = example.replace(r#""mode": "commit-pair""#, r#""mode": "index""#);
    assert!(
        EvaluationRequest::parse(index_with_candidate.as_bytes()).is_err(),
        "an index run has no candidate commit to name"
    );

    let pair_without_candidate = example.replace(
        r#""candidate_commit_oid": "3e19afc65b2704d8ce8b1f09a4de6273550d914b""#,
        r#""candidate_commit_oid": null"#,
    );
    assert!(
        EvaluationRequest::parse(pair_without_candidate.as_bytes()).is_err(),
        "a commit pair with one commit is not a pair"
    );
}

/// The snapshot request carries no discretion. The repository arrives as a fixed
/// handle ordinal the launcher passes, and it is already acquired: a request that
/// asks the engine to open a path, or to go and fetch the repository itself,
/// would hand the evaluator exactly the two capabilities the sandbox exists to
/// take away.
#[test]
fn the_snapshot_request_pins_the_handle_and_the_pre_acquisition() {
    let example = String::from_utf8(request_example("scanner-snapshot-request.json")).unwrap();
    assert_eq!(REPOSITORY_HANDLE_ORDINAL, 3);

    let other_handle = example.replace(r#""repository_handle": 3"#, r#""repository_handle": 4"#);
    assert!(
        SnapshotRequest::parse(other_handle.as_bytes()).is_err(),
        "the handle ordinal is the contract, not a parameter"
    );

    let unacquired = example.replace(r#""pre_acquired": true"#, r#""pre_acquired": false"#);
    assert!(
        SnapshotRequest::parse(unacquired.as_bytes()).is_err(),
        "an engine that acquires its own repository is an engine with the network"
    );

    let index = example.replace(
        r#""materialization": "git-objects""#,
        r#""materialization": "index""#,
    );
    assert_eq!(
        SnapshotRequest::parse(index.as_bytes())
            .unwrap()
            .materialization,
        RequestMode::Index,
        "the other lawful materialization"
    );
}

/// Every supplied control names the external source that authorized it, and the
/// set of those sources is closed. A control whose trust source is a string the
/// contract does not know is not a weakly trusted control; it is not a control,
/// and the request carrying it does not parse.
#[test]
fn a_control_from_an_unknown_authority_is_not_a_control() {
    let example = String::from_utf8(request_example("scanner-controls-request.json")).unwrap();

    let forged = example.replace(
        r#""trust_source": "organization-policy""#,
        r#""trust_source": "repository-workflow""#,
    );
    assert!(
        ControlsRequest::parse(forged.as_bytes()).is_err(),
        "the repository under evaluation is never an authority over its own check"
    );

    let empty = br#"{
  "schema": "amiss/scanner-controls-request",
  "organization_floor": null,
  "debt_snapshot": null,
  "waiver_bundle": null,
  "trusted_time": null,
  "execution_constraint": null
}"#;
    assert_eq!(
        ControlsRequest::parse(empty).unwrap(),
        ControlsRequest::default(),
        "supplying no controls is lawful"
    );
}
