use amiss_wire::model::UtcInstant;
use amiss_wire::requests::ControlsRequest;

#[test]
fn instant_json_checks_the_gregorian_calendar_in_every_wire_year() {
    for year in 0_u32..=9999 {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        for (month, day, accepted) in [
            (2, 28, true),
            (2, 29, leap),
            (2, 30, false),
            (4, 31, false),
            (12, 31, true),
        ] {
            let raw = format!("{year:04}-{month:02}-{day:02}T12:34:56Z");
            let encoded = serde_json::to_string(&raw).unwrap();
            assert_eq!(UtcInstant::try_from(raw.clone()).is_ok(), accepted, "{raw}");
            let instant = serde_json::from_str::<UtcInstant>(&encoded);
            assert_eq!(instant.is_ok(), accepted, "{raw}");
            if let Ok(instant) = instant {
                assert_eq!(instant.as_str(), raw);
                assert_eq!(serde_json::to_string(&instant).unwrap(), encoded);
            }
        }
    }
}

#[test]
fn instant_json_requires_a_complete_whole_second_utc_string() {
    for raw in [
        "",
        "2026-07-12T10:00:60Z",
        "2026-07-12T24:00:00Z",
        "2026-07-12T10:60:00Z",
        "2026-07-12t10:00:00z",
        "2026-07-12 10:00:00Z",
        "2026-07-12T10:00:00+00:00",
        "2026-07-12T10:00:00.0Z",
        "2026-07-12T10:00:00",
        "2026-07-12T10:00:00Z\n",
        "2026-XX-12T10:00:00Z",
        "202X-07-12T10:00:00Z",
        "-001-07-12T10:00:00Z",
        "2026-00-01T10:00:00Z",
        "2026-07-00T10:00:00Z",
        "10000-01-01T00:00:00Z",
        "-0001-12-31T23:59:59Z",
        "2026-07-12T10:00:00💡",
        "2026-07-12T10:00:00\0",
    ] {
        let encoded = serde_json::to_string(raw).unwrap();
        assert!(UtcInstant::try_from(raw.to_owned()).is_err(), "{raw:?}");
        assert!(
            serde_json::from_str::<UtcInstant>(&encoded).is_err(),
            "{raw:?}"
        );
    }
    for raw in [
        "null",
        "true",
        "42",
        "[]",
        "{}",
        "[\"2026-07-12T10:00:00Z\"]",
    ] {
        assert!(serde_json::from_str::<UtcInstant>(raw).is_err(), "{raw}");
    }
    let mut previous = None;
    for raw in [
        "0000-01-01T00:00:00Z",
        "1969-12-31T23:59:59Z",
        "1970-01-01T00:00:00Z",
        "9999-12-31T23:59:59Z",
    ] {
        let instant =
            serde_json::from_str::<UtcInstant>(&serde_json::to_string(raw).unwrap()).unwrap();
        assert!(previous.as_ref().is_none_or(|previous| previous < &instant));
        previous = Some(instant);
    }
}

#[test]
fn controls_requests_reject_impossible_supplied_time() {
    let wire = include_str!("../../../../spec/examples/scanner-controls-request.json");
    ControlsRequest::parse(wire.as_bytes()).unwrap();
    for invalid in [
        "2026-02-30T10:00:00Z",
        "2026-07-12T10:00:60Z",
        "2026-07-12T10:00:00+00:00",
    ] {
        let changed = wire.replace("2026-07-12T10:00:00Z", invalid);
        assert_ne!(wire, changed);
        assert!(
            ControlsRequest::parse(changed.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn reports_reject_impossible_time_even_with_matching_payload_digests() {
    use amiss_wire::digest::hb;
    use amiss_wire::report::{PAYLOAD_SCHEMA, model::Evaluation, validate_envelope};

    let (mut report, _) = validate_envelope(include_bytes!(
        "../../../../spec/examples/scanner-report.json"
    ))
    .unwrap();
    let Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("the fixture must have a resolved evaluation");
    };
    evaluation.evaluation_instant =
        Some(UtcInstant::try_from("2026-07-12T10:00:00Z".to_owned()).unwrap());
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    report.payload_digest = hb(PAYLOAD_SCHEMA, payload.as_bytes());
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
    assert_eq!(validate_envelope(wire.as_bytes()).unwrap().0, report);
    for invalid in [
        "2026-02-30T10:00:00Z",
        "2026-07-12T10:00:60Z",
        "2026-07-12T10:00:00+00:00",
    ] {
        let changed_payload = payload.replace("2026-07-12T10:00:00Z", invalid);
        assert_ne!(changed_payload, payload);
        let changed = wire.replace(&payload, &changed_payload).replace(
            &report.payload_digest.to_string(),
            &hb(PAYLOAD_SCHEMA, changed_payload.as_bytes()).to_string(),
        );
        assert!(validate_envelope(changed.as_bytes()).is_err(), "{invalid}");
    }
}

#[test]
fn trusted_time_lifetime_preserves_calendar_boundaries_and_whole_seconds() {
    use amiss_wire::controls::{canonical_trusted_time, parse_trusted_time};
    use amiss_wire::de::ErrorKind;

    let mut statement = parse_trusted_time(include_bytes!(
        "../../../../spec/examples/scanner-trusted-time-statement.json"
    ))
    .unwrap();
    for (start, end) in [
        ("0000-01-01T00:00:00Z", "0000-01-01T00:10:00Z"),
        ("0000-02-28T23:55:00Z", "0000-02-29T00:05:00Z"),
        ("1900-02-28T23:55:00Z", "1900-03-01T00:05:00Z"),
        ("1969-12-31T23:55:00Z", "1970-01-01T00:05:00Z"),
        ("1999-12-31T23:55:00Z", "2000-01-01T00:05:00Z"),
        ("2000-02-28T23:55:00Z", "2000-02-29T00:05:00Z"),
        ("2000-02-29T23:55:00Z", "2000-03-01T00:05:00Z"),
        ("2100-02-28T23:55:00Z", "2100-03-01T00:05:00Z"),
        ("9999-12-31T23:49:59Z", "9999-12-31T23:59:59Z"),
    ] {
        statement.evaluation_instant = UtcInstant::try_from(start.to_owned()).unwrap();
        statement.valid_until = UtcInstant::try_from(end.to_owned()).unwrap();
        let (bytes, _) = canonical_trusted_time(&statement).unwrap();
        assert_eq!(parse_trusted_time(&bytes).unwrap(), statement);
        std::mem::swap(
            &mut statement.evaluation_instant,
            &mut statement.valid_until,
        );
        assert!(
            canonical_trusted_time(&statement).is_err(),
            "{start} -> {end}"
        );
    }
    statement.evaluation_instant = UtcInstant::try_from("2026-07-12T10:00:00Z".to_owned()).unwrap();
    for (end, accepted) in [
        ("10:00:00", false),
        ("10:00:01", true),
        ("10:09:59", true),
        ("10:10:00", true),
        ("10:10:01", false),
    ] {
        statement.valid_until = UtcInstant::try_from(format!("2026-07-12T{end}Z")).unwrap();
        let encoded = serde_json::to_vec(&statement).unwrap();
        let read = parse_trusted_time(&encoded);
        assert_eq!(read.is_ok(), accepted, "{end}");
        assert_eq!(
            canonical_trusted_time(&statement).is_ok(),
            accepted,
            "{end}"
        );
        if let Err(error) = read {
            assert_eq!(error.path, "$.valid_until");
            assert!(matches!(error.kind, ErrorKind::InvalidValue));
        }
    }
}
