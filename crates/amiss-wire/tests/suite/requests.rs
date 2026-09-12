#![expect(
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use sha2::Digest as _;
use std::fs;
use std::path::Path;

use amiss_wire::controls::Profile;
use amiss_wire::de::ErrorKind;

use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use amiss_wire::requests::{
    CANDIDATE_IDENTITY_DOMAIN, ControlsRequest, EvaluationRequest, EvaluationRequestSchema,
    REPOSITORY_HANDLE_ORDINAL, REQUEST_STREAM_BYTES, RequestMode, RequestStreams, RequestTrust,
    SEMANTIC_EVIDENCE_REQUEST_LIMIT, SnapshotMaterialization, SnapshotRequest,
    SuppliedSemanticEvidence, commit_candidate_identity_digest,
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
    assert_eq!(evaluation.schema, EvaluationRequestSchema::Current);
    assert_eq!(evaluation.profile, Profile::Enforce);
    assert_eq!(evaluation.mode, RequestMode::CommitPair);
    assert_eq!(evaluation.object_format, ObjectFormat::Sha1);
    let repository = evaluation
        .repository
        .expect("the example names a repository");
    assert_eq!(repository.host(), "gitlab.example.internal");
    assert_eq!(repository.owner(), "platform/security");
    assert_eq!(repository.name(), "docs");
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

    let snapshot = serde_json::from_slice::<SnapshotRequest>(&request_example(
        "scanner-snapshot-request.json",
    ))
    .unwrap();
    assert_eq!(
        snapshot.materialization,
        SnapshotMaterialization::GitObjects
    );

    let controls =
        ControlsRequest::parse(&request_example("scanner-controls-request.json")).unwrap();
    let floor = controls
        .organization_floor
        .as_ref()
        .expect("the example supplies one");
    assert_eq!(floor.trust_source, RequestTrust::OrganizationPolicy);
    assert_eq!(
        floor.expected_digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/organization-floor")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&floor.value).unwrap())
                .finalize()
                .0
        ),
        "the request carries the floor's independently reproducible semantic digest"
    );
    let time = controls
        .trusted_time
        .as_ref()
        .expect("the example supplies a trusted instant");
    assert_eq!(time.provider, "gitlab");
    assert_eq!(time.provider_run_id, "pipeline/987654321:job-42");
    assert_eq!(time.provider_run_attempt, 2);
    assert_eq!(
        time.expected_digest,
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-trusted-time-statement")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&time.value).unwrap())
                .finalize()
                .0
        ),
        "the request carries the statement's independently reproducible semantic digest"
    );
    assert!(
        controls.debt_snapshot.is_none()
            && controls.waiver_bundle.is_none()
            && controls.execution_constraint.is_none()
            && controls.semantic_evidence.is_empty(),
        "an absent control is absent, never a default"
    );
}

#[test]
fn control_readers_reject_unknown_payloads_within_the_json_depth_limit() {
    let example = String::from_utf8(request_example("scanner-controls-request.json")).unwrap();
    for depth in [0, 124, 125] {
        let invalid = example.replacen(
            "\"floor_id\":",
            &format!(
                "\"future\": {}null{}, \"floor_id\":",
                "[".repeat(depth),
                "]".repeat(depth)
            ),
            1,
        );
        assert_ne!(invalid, example);
        amiss_wire::de::JsonProfile::validate(invalid.as_bytes()).unwrap();
        assert!(serde_json::from_str::<ControlsRequest>(&invalid).is_err());
        assert_eq!(
            ControlsRequest::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::UnknownField
        );
    }
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

    let published =
        serde_json::from_slice::<serde_json::Value>(&request_example("candidate-identity.json"))
            .expect("the candidate identity example is strict JSON");
    assert_eq!(
        commit_candidate_identity_digest(&evaluation, &oid('2'), &oid('4')),
        Some(amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(CANDIDATE_IDENTITY_DOMAIN)
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&published).expect("fixture JSON"))
                .finalize()
                .0
        ))
    );

    let mut mismatched = evaluation.clone();
    mismatched.object_format = ObjectFormat::Sha256;
    assert_eq!(
        commit_candidate_identity_digest(&mismatched, &oid('2'), &oid('4')),
        None,
        "identity construction validates commit formats without serializing the request"
    );
    mismatched = evaluation.clone();
    mismatched.target_ref = None;
    assert_eq!(
        commit_candidate_identity_digest(&mismatched, &oid('2'), &oid('4')),
        None,
        "identity construction rejects incomplete repository identity"
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
        serde_json::from_slice::<SnapshotRequest>(wrong_schema.as_bytes()).is_err(),
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
    let snapshot = serde_json::from_slice::<SnapshotRequest>(&request_example(
        "scanner-snapshot-request.json",
    ))
    .unwrap();
    let controls =
        ControlsRequest::parse(&request_example("scanner-controls-request.json")).unwrap();
    let streams = RequestStreams {
        evaluation: serde_json_canonicalizer::to_vec(&evaluation).unwrap(),
        snapshot: serde_json_canonicalizer::to_vec(&snapshot).unwrap(),
        controls: serde_json_canonicalizer::to_vec(&controls).unwrap(),
    };
    for bytes in [&streams.evaluation, &streams.snapshot, &streams.controls] {
        assert_eq!(
            serde_json_canonicalizer::to_vec(
                &serde_json::from_slice::<serde_json::Value>(bytes).unwrap()
            )
            .unwrap(),
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
        let error = invalid.validate().unwrap_err();
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

#[test]
fn nullable_evaluation_members_are_required() {
    let example: serde_json::Value =
        serde_json::from_slice(&request_example("scanner-evaluation-request.json")).unwrap();
    for field in [
        "repository",
        "forge",
        "candidate_ref",
        "target_ref",
        "default_branch_ref",
        "candidate_commit_oid",
    ] {
        let mut missing = example.clone();
        missing.as_object_mut().unwrap().remove(field);
        let error = EvaluationRequest::parse(&serde_json::to_vec(&missing).unwrap()).unwrap_err();
        assert_eq!(error.path, format!("$.{field}"));
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
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
        serde_json::from_slice::<SnapshotRequest>(other_handle.as_bytes())
            .unwrap()
            .validate()
            .is_err(),
        "the handle ordinal is the contract, not a parameter"
    );

    let unacquired = example.replace(r#""pre_acquired": true"#, r#""pre_acquired": false"#);
    assert!(
        serde_json::from_slice::<SnapshotRequest>(unacquired.as_bytes())
            .unwrap()
            .validate()
            .is_err(),
        "an engine that acquires its own repository is an engine with the network"
    );

    for (request, path) in [
        (
            SnapshotRequest {
                repository_handle: 4,
                ..SnapshotRequest::git_objects()
            },
            "$.repository_handle",
        ),
        (
            SnapshotRequest {
                pre_acquired: false,
                ..SnapshotRequest::git_objects()
            },
            "$.pre_acquired",
        ),
    ] {
        let error = request.validate().unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }

    let index = example.replace(
        r#""materialization": "git-objects""#,
        r#""materialization": "index""#,
    );
    assert_eq!(
        serde_json::from_slice::<SnapshotRequest>(index.as_bytes())
            .unwrap()
            .materialization,
        SnapshotMaterialization::Index,
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
  "execution_constraint": null,
  "semantic_evidence": []
}"#;
    assert_eq!(
        ControlsRequest::parse(empty).unwrap(),
        ControlsRequest::default(),
        "supplying no controls is lawful"
    );
    for field in [
        "organization_floor",
        "debt_snapshot",
        "waiver_bundle",
        "trusted_time",
        "execution_constraint",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(empty).unwrap();
        value.as_object_mut().unwrap().remove(field);
        let error = ControlsRequest::parse(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert_eq!(error.path, format!("$.{field}"));
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
}

#[test]
fn semantic_evidence_is_a_bounded_set_of_envelopes() {
    let value = amiss_wire::semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let supplied = SuppliedSemanticEvidence {
        expected_context_digest: value.payload.producer.context_digest,
        value,
    };
    let oversized = ControlsRequest {
        semantic_evidence: vec![supplied; SEMANTIC_EVIDENCE_REQUEST_LIMIT.saturating_add(1)],
        ..ControlsRequest::default()
    };
    let error = oversized.validate().unwrap_err();
    assert_eq!(error.path, "$.semantic_evidence");
    assert_eq!(error.kind, ErrorKind::LimitExceeded);
}

/// Both words of the mode vocabulary and both of the object-format vocabulary
/// are requests the engine has to accept, not just the pair the example spells.
#[test]
fn the_other_lawful_evaluation_words_parse() {
    let example = String::from_utf8(request_example("scanner-evaluation-request.json")).unwrap();

    let staged = example
        .replace(r#""mode": "commit-pair""#, r#""mode": "index""#)
        .replace(
            r#""candidate_commit_oid": "3e19afc65b2704d8ce8b1f09a4de6273550d914b""#,
            r#""candidate_commit_oid": null"#,
        );
    let request = EvaluationRequest::parse(staged.as_bytes()).expect("a staged run is lawful");
    assert_eq!(request.mode, RequestMode::Index);
    assert!(request.candidate_commit.is_none());

    let wider = example
        .replace(r#""object_format": "sha1""#, r#""object_format": "sha256""#)
        .replace("8d7f2c31a09b64e5dd10fcab7e93245160c8ba72", &"a".repeat(64))
        .replace("3e19afc65b2704d8ce8b1f09a4de6273550d914b", &"b".repeat(64));
    let request = EvaluationRequest::parse(wider.as_bytes()).expect("the other object format");
    assert_eq!(request.object_format, ObjectFormat::Sha256);
    assert_eq!(request.base_commit.as_str(), "a".repeat(64));
}

/// The other authority in the closed pair is an authority.
#[test]
fn a_control_from_the_required_check_is_a_control() {
    let example = String::from_utf8(request_example("scanner-controls-request.json")).unwrap();
    let checked = example.replace(
        r#""trust_source": "organization-policy""#,
        r#""trust_source": "external-required-check""#,
    );
    let request = ControlsRequest::parse(checked.as_bytes()).expect("both sources are lawful");
    assert_eq!(
        request
            .organization_floor
            .expect("the example supplies a floor")
            .trust_source,
        RequestTrust::ExternalRequiredCheck
    );
}

/// The stream ceiling is a ceiling: a request that reaches it is written and
/// read, and one byte more is refused on both sides of the frame.
#[test]
fn a_stream_may_reach_the_ceiling_and_not_pass_it() {
    let ceiling = usize::try_from(REQUEST_STREAM_BYTES).expect("the ceiling fits this host");
    let at_ceiling = RequestStreams {
        evaluation: vec![b' '; ceiling],
        snapshot: Vec::new(),
        controls: Vec::new(),
    };
    let mut frame = Vec::new();
    at_ceiling
        .write_to(&mut frame)
        .expect("a stream of exactly the ceiling is writable");
    assert_eq!(
        RequestStreams::read_from(&mut frame.as_slice()).expect("and readable"),
        at_ceiling
    );

    let past_ceiling = RequestStreams {
        evaluation: vec![b' '; ceiling.saturating_add(1)],
        snapshot: Vec::new(),
        controls: Vec::new(),
    };
    assert_eq!(
        past_ceiling
            .write_to(&mut Vec::new())
            .expect_err("one byte past the ceiling is not writable")
            .kind(),
        std::io::ErrorKind::InvalidData
    );

    let declared = |length: u64| {
        let mut claim = Vec::new();
        claim.extend_from_slice(b"AMISSRQ1");
        claim.extend_from_slice(&length.to_be_bytes());
        claim
    };
    assert_eq!(
        RequestStreams::read_from(&mut declared(REQUEST_STREAM_BYTES).as_slice())
            .expect_err("the body is missing")
            .kind(),
        std::io::ErrorKind::UnexpectedEof,
        "a declared length at the ceiling is a length the reader accepts"
    );
    assert_eq!(
        RequestStreams::read_from(&mut declared(REQUEST_STREAM_BYTES.saturating_add(1)).as_slice())
            .expect_err("one byte past the ceiling")
            .kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn constructed_controls_keep_the_safe_integer_law_without_validating_control_meaning() {
    let original =
        ControlsRequest::parse(&request_example("scanner-controls-request.json")).unwrap();
    let mut request = original.clone();
    request
        .organization_floor
        .as_mut()
        .unwrap()
        .value
        .resource_limits = vec![amiss_wire::controls::ResourceLimit {
        resource: amiss_wire::controls::ResourceName::MachineJsonBytes,
        maximum: 0,
    }];
    for maximum in [
        i64::MIN,
        -9_007_199_254_740_992,
        9_007_199_254_740_992,
        i64::MAX,
    ] {
        request
            .organization_floor
            .as_mut()
            .unwrap()
            .value
            .resource_limits[0]
            .maximum = maximum;
        let error = request.validate().unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidValue);
        assert_eq!(
            error.path,
            "$.organization_floor.value.resource_limits[0].maximum"
        );
    }
    request
        .organization_floor
        .as_mut()
        .unwrap()
        .value
        .resource_limits[0]
        .maximum = -js_int::MAX_SAFE_INT;
    assert!(
        request.validate().is_ok(),
        "a semantically invalid floor belongs to the control consumer"
    );
    request = original;
    for attempt in [9_007_199_254_740_992, u64::MAX] {
        request
            .trusted_time
            .as_mut()
            .unwrap()
            .value
            .provider_run_attempt = attempt;
        let error = request.validate().unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidValue);
        assert_eq!(error.path, "$.trusted_time.value.provider_run_attempt");
    }
    request
        .trusted_time
        .as_mut()
        .unwrap()
        .value
        .provider_run_attempt = 0;
    assert!(
        request.validate().is_ok(),
        "the supplied statement is validated by its consumer"
    );
}

#[test]
fn supplied_fact_multiplicity_keeps_numeric_and_semantic_validation_separate() {
    for (name, field, fact_field) in [
        ("debt-snapshot.json", "debt_snapshot", "accepted_fact"),
        ("waiver-bundle.json", "waiver_bundle", "authorized_fact"),
    ] {
        let control: serde_json::Value = serde_json::from_slice(&request_example(name)).unwrap();
        let mut value = serde_json::to_value(ControlsRequest::default()).unwrap();
        value[field] = serde_json::json!({
            "value": control,
            "expected_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "trust_source": "organization-policy"
        });
        let pointer =
            format!("/{field}/value/items/0/{fact_field}/evidence/occurrence_multiplicity");
        for multiplicity in [9_007_199_254_740_992_u64, u64::MAX] {
            *value.pointer_mut(&pointer).unwrap() = serde_json::json!(multiplicity);
            let request: ControlsRequest = serde_json::from_value(value.clone()).unwrap();
            let error = request.validate().unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidValue);
            assert_eq!(
                error.path,
                format!("$.{field}.value.items[0].{fact_field}.evidence.occurrence_multiplicity")
            );
        }
        *value.pointer_mut(&pointer).unwrap() = serde_json::json!(2);
        let request: ControlsRequest = serde_json::from_value(value).unwrap();
        assert!(
            request.validate().is_ok(),
            "multiplicity semantics remain a fact constraint"
        );
    }
}

#[test]
fn request_models_refuse_positional_root_and_supplied_objects() {
    let snapshot = serde_json::json!(["amiss/scanner-snapshot-request", "git-objects", 3, true]);
    assert!(serde_json::from_value::<SnapshotRequest>(snapshot).is_err());
    let controls: serde_json::Value =
        serde_json::from_slice(&request_example("scanner-controls-request.json")).unwrap();
    let root = serde_json::json!([
        controls["schema"],
        controls["organization_floor"],
        controls["debt_snapshot"],
        controls["waiver_bundle"],
        controls["trusted_time"],
        controls["execution_constraint"],
        controls["semantic_evidence"]
    ]);
    assert!(ControlsRequest::parse(&serde_json::to_vec(&root).unwrap()).is_err());
    for (field, fields) in [
        (
            "organization_floor",
            &["value", "expected_digest", "trust_source"][..],
        ),
        (
            "trusted_time",
            &[
                "value",
                "expected_digest",
                "provider",
                "provider_run_id",
                "provider_run_attempt",
            ][..],
        ),
    ] {
        let mut malformed = controls.clone();
        malformed[field] = fields
            .iter()
            .map(|key| controls[field][key].clone())
            .collect();
        assert!(
            ControlsRequest::parse(&serde_json::to_vec(&malformed).unwrap()).is_err(),
            "{field}"
        );
        assert!(
            serde_json::from_value::<ControlsRequest>(malformed).is_err(),
            "{field}"
        );
    }
    let evaluation: serde_json::Value =
        serde_json::from_slice(&request_example("scanner-evaluation-request.json")).unwrap();
    let root: serde_json::Value = [
        "schema",
        "profile",
        "mode",
        "object_format",
        "repository",
        "forge",
        "candidate_ref",
        "target_ref",
        "default_branch_ref",
        "base_commit_oid",
        "candidate_commit_oid",
    ]
    .iter()
    .map(|key| evaluation[key].clone())
    .collect();
    assert!(EvaluationRequest::parse(&serde_json::to_vec(&root).unwrap()).is_err());
    assert!(serde_json::from_value::<EvaluationRequest>(root).is_err());
}
