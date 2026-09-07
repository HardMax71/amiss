use amiss_wire::digest::hb;
use amiss_wire::report::{PAYLOAD_SCHEMA, ReportDefect, validate_envelope};
use serde_json::{Value, json};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn additive_fields_are_checked_before_typed_decoding() {
    let original = validate_envelope(REPORT).unwrap();
    for path in ["/payload", "/payload/engine", "/payload/summary"] {
        let mut report: Value = serde_json::from_slice(REPORT).unwrap();
        report.pointer_mut(path).unwrap()["future_field"] =
            json!({"\u{1f600}": [null, true, -7], "\u{e000}": "extra"});
        assert_eq!(
            validate_envelope(&serde_json::to_vec(&report).unwrap()).map(|_| ()),
            Err(ReportDefect::DigestMismatch),
            "{path}"
        );
        let canonical = bind(&mut report).unwrap();
        let (payload, digest, verdict) = validate_envelope(&canonical).unwrap();
        assert_eq!(payload, original.0, "{path}");
        assert_ne!(digest, original.1, "{path}");
        assert_eq!(verdict, original.2);
        assert_eq!(
            validate_envelope(&serde_json::to_vec_pretty(&report).unwrap()).unwrap(),
            (payload, digest, verdict)
        );
    }
}

#[test]
fn the_outer_envelope_rejects_members_outside_the_payload_digest() {
    let original: Value = serde_json::from_slice(REPORT).unwrap();
    let schema: Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/scanner-report.schema.json"
    ))
    .unwrap();
    assert_eq!(schema["additionalProperties"], false);
    validate_envelope(REPORT).unwrap();
    for extra in [Value::Null, json!(false), json!(1), json!({"nested": []})] {
        let mut report = original.clone();
        report["future_field"] = extra;
        let bytes = serde_json::to_vec(&report).unwrap();
        assert_eq!(
            validate_envelope(&bytes).map(|_| ()),
            Err(ReportDefect::NotAReport)
        );
    }
}

#[test]
fn report_headers_and_verdicts_keep_their_closed_json_shapes() {
    for (path, invalid, expected) in [
        (
            "/schema",
            json!({"amiss/scanner-report-envelope": null}),
            ReportDefect::NotAReport,
        ),
        (
            "/payload/schema",
            json!({"amiss/scanner-report-payload": null}),
            ReportDefect::NotAReport,
        ),
        (
            "/payload/compatibility",
            json!("2"),
            ReportDefect::UnsupportedCompatibility,
        ),
        (
            "/payload/compatibility",
            json!({"1": null}),
            ReportDefect::NotAReport,
        ),
        (
            "/payload_digest",
            json!("not-a-digest"),
            ReportDefect::NotAReport,
        ),
        ("/payload", json!([]), ReportDefect::NotAReport),
        (
            "/payload/result",
            json!([true, "pass", 0]),
            ReportDefect::InvalidResult,
        ),
        (
            "/payload/result/status",
            json!({"pass": null}),
            ReportDefect::InvalidResult,
        ),
        (
            "/payload/result/complete",
            json!(null),
            ReportDefect::InvalidResult,
        ),
        (
            "/payload/result/exit_code",
            json!(2),
            ReportDefect::InvalidResult,
        ),
        (
            "/payload/result/status",
            json!("fail"),
            ReportDefect::InvalidResult,
        ),
    ] {
        let mut report: Value = serde_json::from_slice(REPORT).unwrap();
        *report.pointer_mut(path).unwrap() = invalid;
        let bytes = if path == "/payload_digest" {
            serde_json::to_vec(&report).unwrap()
        } else {
            bind(&mut report).unwrap()
        };
        assert_eq!(
            validate_envelope(&bytes).map(|_| ()),
            Err(expected),
            "{path}"
        );
    }
    let report: Value = serde_json::from_slice(REPORT).unwrap();
    let array = json!([
        report["payload"],
        report["payload_digest"],
        report["schema"]
    ]);
    assert_eq!(
        validate_envelope(&serde_json::to_vec(&array).unwrap()),
        Err(ReportDefect::NotAReport)
    );
}

#[test]
fn unowned_fields_keep_the_strict_number_duplicate_and_stream_rules() {
    let report = std::str::from_utf8(REPORT).unwrap();
    for inserted in [
        r#""future":-0,"#,
        r#""future":1.0,"#,
        r#""future":1e0,"#,
        r#""future":9007199254740992,"#,
        r#""future":0,"future":0,"#,
        r#""future":0,"\u0066uture":0,"#,
    ] {
        let changed = report.replacen(r#""payload":{"#, &format!(r#""payload":{{{inserted}"#), 1);
        assert_eq!(
            validate_envelope(changed.as_bytes()).map(|_| ()),
            Err(ReportDefect::NotAReport)
        );
    }
    for extra in ["null", "{}", "garbage"] {
        let changed = format!("{report}{extra}");
        assert_eq!(
            validate_envelope(changed.as_bytes()).map(|_| ()),
            Err(ReportDefect::NotAReport)
        );
    }
}

#[test]
fn opaque_payload_fields_keep_the_existing_depth_ceiling() {
    let mut report: Value = serde_json::from_slice(REPORT).unwrap();
    let mut nested = Value::Null;
    for _ in 0..256 {
        nested = json!([nested]);
    }
    report["payload"]["future_field"] = nested.clone();
    validate_envelope(&bind(&mut report).unwrap()).unwrap();
    for _ in 256..513 {
        nested = json!([nested]);
    }
    report["payload"]["future_field"] = nested;
    assert_eq!(
        validate_envelope(&bind(&mut report).unwrap()).map(|_| ()),
        Err(ReportDefect::NotAReport)
    );
}

fn bind(report: &mut Value) -> serde_json::Result<Vec<u8>> {
    let payload = serde_json_canonicalizer::to_vec(&report["payload"])?;
    report["payload_digest"] = json!(hb(PAYLOAD_SCHEMA, &payload));
    serde_json_canonicalizer::to_vec(report)
}
