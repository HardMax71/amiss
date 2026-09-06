use amiss_wire::{controls::canonical_trusted_time, requests::ControlsRequest};

#[test]
fn supplied_time_is_a_closed_object_and_keeps_its_canonical_identity() {
    let example = include_str!("../../../../spec/examples/scanner-controls-request.json");
    let request = ControlsRequest::parse(example.as_bytes()).unwrap();
    let supplied = request.trusted_time.as_ref().unwrap();
    assert_eq!(
        canonical_trusted_time(&supplied.value).unwrap().1,
        supplied.expected_digest
    );
    let canonical = request.canonical_bytes().unwrap();
    assert_eq!(ControlsRequest::parse(&canonical).unwrap(), request);

    for (original, replacement) in [
        ("\"controller\":", "\"future\": true, \"controller\":"),
        ("amiss/scanner-trusted-time-statement", "amiss/future-time"),
        ("external-required-check-clock", "future-clock"),
        ("\"controller\": \"external-required-check-clock\",", ""),
        (
            "\"controller\": \"external-required-check-clock\"",
            "\"controller\": null",
        ),
    ] {
        let invalid = example.replace(original, replacement);
        assert_ne!(invalid, example);
        assert!(
            ControlsRequest::parse(invalid.as_bytes()).is_err(),
            "{replacement}"
        );
        assert!(
            serde_json::from_str::<ControlsRequest>(&invalid).is_err(),
            "{replacement}"
        );
    }

    let statement = &supplied.value;
    let object = serde_json::to_string(statement).unwrap();
    let positional = serde_json::to_string(&(
        statement.candidate_identity_digest,
        statement.controller,
        &statement.evaluation_instant,
        &statement.provider,
        statement.provider_run_attempt,
        &statement.provider_run_id,
        &statement.ref_name,
        &statement.repository,
        statement.schema,
        &statement.valid_until,
    ))
    .unwrap();
    let compact = serde_json::to_string(&request).unwrap();
    for invalid in [
        positional.as_str(),
        "null",
        "[]",
        "{}",
        "true",
        "42",
        "\"time\"",
    ] {
        let altered = compact.replace(&object, invalid);
        assert_ne!(altered, compact);
        assert!(
            ControlsRequest::parse(altered.as_bytes()).is_err(),
            "{invalid}"
        );
        assert!(
            serde_json::from_str::<ControlsRequest>(&altered).is_err(),
            "{invalid}"
        );
    }
}
