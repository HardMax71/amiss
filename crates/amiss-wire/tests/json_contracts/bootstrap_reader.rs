use amiss_wire::{
    controls::{self, ExecutionConstraintDescriptor, parse_execution_constraint},
    de::ErrorKind,
    manifest,
    requests::{self, ControlsRequest, EvaluationRequest, SnapshotRequest},
};

const EVALUATION: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-evaluation-request.json");
const SNAPSHOT: &[u8] = include_bytes!("../../../../spec/examples/scanner-snapshot-request.json");
const CONSTRAINT: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-execution-constraint.json");

#[test]
fn bootstrap_readers_reject_malformed_complete_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let readers: [fn(&[u8]) -> bool; 5] = [
        |bytes| EvaluationRequest::parse(bytes).is_ok(),
        |bytes| SnapshotRequest::parse(bytes).is_ok(),
        |bytes| parse_execution_constraint(bytes).is_ok(),
        |bytes| manifest::parse_release_manifest(bytes).is_ok(),
        |bytes| ControlsRequest::parse(bytes).is_ok(),
    ];
    for ((bytes, schema), read) in [
        (EVALUATION, requests::EVALUATION_REQUEST_SCHEMA),
        (SNAPSHOT, requests::SNAPSHOT_REQUEST_SCHEMA),
        (CONSTRAINT, controls::EXECUTION_CONSTRAINT_SCHEMA),
        (
            include_bytes!("../../../../spec/examples/scanner-release-manifest.json").as_slice(),
            manifest::MANIFEST_DOMAIN,
        ),
        (
            include_bytes!("../../../../spec/examples/scanner-controls-request.json").as_slice(),
            requests::CONTROLS_REQUEST_SCHEMA,
        ),
    ]
    .into_iter()
    .zip(readers)
    {
        super::input::assert_closed_input(bytes, schema, None, read)?;
    }
    Ok(())
}

#[test]
fn evaluation_reader_requires_an_object_repository() -> Result<(), Box<dyn std::error::Error>> {
    let request = EvaluationRequest::parse(EVALUATION)?;
    let repository = request.repository.as_ref().unwrap();
    super::input::assert_object_required(
        (&request, EvaluationRequest::parse),
        repository,
        (repository.host(), repository.name(), repository.owner()),
    )?;
    let no_identity = EvaluationRequest::commit_pair(
        request.profile,
        request.object_format,
        request.base_commit,
        request.candidate_commit.unwrap(),
    );
    let text = serde_json::to_string(&no_identity)?;
    assert_eq!(EvaluationRequest::parse(text.as_bytes())?, no_identity);
    for invalid in ["{}", "[]", "true"] {
        let changed = text.replace("\"repository\":null", &format!("\"repository\":{invalid}"));
        assert_ne!(changed, text);
        assert!(EvaluationRequest::parse(changed.as_bytes()).is_err());
    }
    Ok(())
}

#[test]
fn constraint_repository_is_an_object_in_the_model_and_reader() {
    let descriptor = parse_execution_constraint(CONSTRAINT).unwrap();
    let text = serde_json::to_string(&descriptor).unwrap();
    let repository = &descriptor.action_repository;
    let object = serde_json::to_string(repository).unwrap();
    let positional =
        serde_json::to_string(&(repository.host(), repository.name(), repository.owner())).unwrap();
    let changed = text.replace(&object, &positional);
    assert_ne!(changed, text);
    let error = parse_execution_constraint(changed.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.action_repository");
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    assert!(serde_json::from_str::<ExecutionConstraintDescriptor>(&changed).is_err());
    let (bytes, digest) = controls::canonical_execution_constraint(&descriptor).unwrap();
    let replay = parse_execution_constraint(&bytes).unwrap();
    assert_eq!(replay, descriptor);
    assert_eq!(
        controls::canonical_execution_constraint(&replay).unwrap().1,
        digest
    );
}

#[test]
fn snapshot_handles_reject_unsafe_integers_in_serde() {
    let mut request = SnapshotRequest::parse(SNAPSHOT).unwrap();
    let text = serde_json::to_string(&request).unwrap();
    let maximum = amiss_wire::json::MAX_SAFE_INTEGER;
    for repository_handle in [i64::MIN, -maximum - 1, maximum + 1, i64::MAX] {
        request.repository_handle = repository_handle;
        let changed = text.replace(
            "\"repository_handle\":3",
            &format!("\"repository_handle\":{repository_handle}"),
        );
        assert_ne!(changed, text);
        assert_eq!(
            [
                serde_json::from_str::<SnapshotRequest>(&changed).is_err(),
                serde_json::to_vec(&request).is_err(),
            ],
            [true; 2],
            "{repository_handle}"
        );
    }
    for invalid in ["-0", "3.0", "3e0", "null", "true", "\"3\"", "0", "4"] {
        let changed = text.replace(
            "\"repository_handle\":3",
            &format!("\"repository_handle\":{invalid}"),
        );
        assert_ne!(changed, text);
        assert!(
            SnapshotRequest::parse(changed.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
