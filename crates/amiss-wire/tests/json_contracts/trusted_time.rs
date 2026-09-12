use amiss_wire::{
    controls::{canonical_trusted_time, parse_trusted_time},
    de::ErrorKind,
    requests::ControlsRequest,
};

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
    assert_eq!(parse_trusted_time(object.as_bytes()).unwrap(), *statement);
    assert!(
        matches!(parse_trusted_time(positional.as_bytes()).unwrap_err().kind, ErrorKind::Deserialize(source) if source.is_data())
    );
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

#[test]
fn trusted_time_requires_an_object_repository() {
    let request = ControlsRequest::parse(include_bytes!(
        "../../../../spec/examples/scanner-controls-request.json"
    ))
    .unwrap();
    let statement = request.trusted_time.unwrap().value;
    let encoded = serde_json::to_string(&statement).unwrap();
    let repository = serde_json::to_string(&statement.repository).unwrap();
    let positional = serde_json::to_string(&(
        statement.repository.host(),
        statement.repository.name(),
        statement.repository.owner(),
    ))
    .unwrap();
    let invalid = encoded.replace(&repository, &positional);
    assert_ne!(invalid, encoded);
    let error = parse_trusted_time(invalid.as_bytes()).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    assert_eq!(error.path, "$.repository");
    assert!(serde_json::from_str::<amiss_wire::controls::TrustedTimeStatement>(&invalid).is_err());
}

#[test]
fn trusted_time_reader_keeps_complete_input_and_checked_attempts() {
    let example = include_str!("../../../../spec/examples/scanner-trusted-time-statement.json");
    let mut statement = parse_trusted_time(example.as_bytes()).unwrap();
    for attempt in [1, amiss_wire::json::MAX_SAFE_INTEGER.unsigned_abs()] {
        statement.provider_run_attempt = attempt;
        let (bytes, digest) = canonical_trusted_time(&statement).unwrap();
        let parsed = parse_trusted_time(&bytes).unwrap();
        assert_eq!(parsed, statement);
        assert_eq!(canonical_trusted_time(&parsed).unwrap().1, digest);
    }

    for suffix in ["{}", "[]", "true", "0", "]"] {
        let invalid = format!("{example}{suffix}");
        let error = parse_trusted_time(invalid.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$");
        assert!(matches!(
            error.kind,
            ErrorKind::Deserialize(source) if source.is_syntax()
        ));
    }
    assert!(parse_trusted_time(format!(" \n{example}\r\t").as_bytes()).is_ok());
    for invalid in [
        b"\xff".as_slice(),
        b"\xef\xbb\xbf{}",
        b"null",
        b"true",
        b"0",
    ] {
        assert!(parse_trusted_time(invalid).is_err(), "{invalid:?}");
    }
    for attempt in ["0", "-0", "2.0", "2e0", "9007199254740992"] {
        let invalid = example.replace(
            "\"provider_run_attempt\": 2",
            &format!("\"provider_run_attempt\": {attempt}"),
        );
        assert_ne!(invalid, example);
        let error = parse_trusted_time(invalid.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.provider_run_attempt", "{attempt}");
    }
    for field in [
        r#""provider_run_attempt":2,"#,
        r#""provider_run_\u0061ttempt":2,"#,
        r#""future":null,"#,
    ] {
        let invalid = example.replacen('{', &format!("{{{field}"), 1);
        assert!(parse_trusted_time(invalid.as_bytes()).is_err(), "{field}");
    }
}
