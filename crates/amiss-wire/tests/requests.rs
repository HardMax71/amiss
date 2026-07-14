#![expect(
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_wire::controls::Profile;
use amiss_wire::model::ObjectFormat;
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, REPOSITORY_HANDLE_ORDINAL, RequestMode, RequestTrust,
    SnapshotRequest,
};

/// The dossier's own canonical instance of a request, in this implementation's
/// namespace. Reading the frozen example rather than a copy of it is the point:
/// a hand-kept fixture drifts from the specification it was copied out of, and
/// nothing notices. The wire strings are the only difference, and they are
/// rewritten wholesale.
fn dossier_request(name: &str) -> Vec<u8> {
    let raw = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .expect("the dossier ships this example");
    String::from_utf8(raw)
        .expect("the example is UTF-8")
        .replace("assure/", "amiss/")
        .into_bytes()
}

/// The three request contracts are the only shapes in amiss-wire with no
/// consumer: no wrapper API is authorized in v0, so nothing in the workspace
/// constructs or parses one, and until now nothing tested one either. They are
/// the interface the request-wire RFC has to land against, and an untested
/// parser is a specification nobody has checked the code against.
#[test]
fn the_frozen_request_examples_parse_to_what_they_say() {
    let evaluation =
        EvaluationRequest::parse(&dossier_request("scanner-evaluation-request-v1.json")).unwrap();
    assert_eq!(evaluation.profile, Profile::Enforce);
    assert_eq!(evaluation.mode, RequestMode::CommitPair);
    assert_eq!(evaluation.object_format, ObjectFormat::Sha1);
    let repository = evaluation
        .repository
        .expect("the example names a repository");
    assert_eq!(repository.owner, "acme");
    assert_eq!(repository.name, "spec-to-rest");
    assert_eq!(
        evaluation.base_commit.as_str(),
        "8d7f2c31a09b64e5dd10fcab7e93245160c8ba72"
    );
    assert_eq!(
        evaluation
            .candidate_commit
            .as_ref()
            .map(amiss_wire::model::Oid::as_str),
        Some("3e19afc65b2704d8ce8b1f09a4de6273550d914b"),
        "a commit-pair run names both sides"
    );

    let snapshot =
        SnapshotRequest::parse(&dossier_request("scanner-snapshot-request-v1.json")).unwrap();
    assert_eq!(snapshot.materialization, RequestMode::CommitPair);

    let controls =
        ControlsRequest::parse(&dossier_request("scanner-controls-request-v1.json")).unwrap();
    let floor = controls
        .organization_floor
        .expect("the example supplies one");
    assert_eq!(floor.trust_source, RequestTrust::OrganizationRuleset);
    let time = controls.trusted_time.expect("and a trusted instant");
    assert_eq!(time.provider_run_id, "987654321");
    assert_eq!(time.provider_run_attempt, 2);
    assert!(
        controls.debt_snapshot.is_none()
            && controls.waiver_bundle.is_none()
            && controls.execution_constraint.is_none(),
        "an absent control is absent, never a default"
    );
}

/// The candidate commit is null exactly when the mode is `index`. Both ways of
/// getting that wrong describe a run that cannot exist: a commit pair with one
/// side, or a staged index that also names a candidate commit. Neither is a
/// request the engine could act on, and neither parses.
#[test]
fn the_evaluation_request_binds_the_candidate_to_the_mode() {
    let example = String::from_utf8(dossier_request("scanner-evaluation-request-v1.json")).unwrap();

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
    let example = String::from_utf8(dossier_request("scanner-snapshot-request-v1.json")).unwrap();
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
    let example = String::from_utf8(dossier_request("scanner-controls-request-v1.json")).unwrap();

    let forged = example.replace(
        r#""trust_source": "organization-ruleset""#,
        r#""trust_source": "repository-workflow""#,
    );
    assert!(
        ControlsRequest::parse(forged.as_bytes()).is_err(),
        "the repository under evaluation is never an authority over its own check"
    );

    let empty = br#"{
  "schema": "amiss/scanner-controls-request/v1",
  "organization_floor": null,
  "debt_snapshot": null,
  "waiver_bundle": null,
  "trusted_time": null,
  "execution_constraint": null
}"#;
    assert_eq!(
        ControlsRequest::parse(empty).unwrap(),
        ControlsRequest::default(),
        "supplying no controls is lawful, and is what every v0 run does"
    );
}
