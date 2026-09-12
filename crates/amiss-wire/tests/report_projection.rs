use std::collections::BTreeSet;

use amiss_wire::codec;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json;
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, PAYLOAD_SCHEMA, engine_block, invocation_failure_envelope,
    sandbox_descriptor,
};

#[test]
fn borrowed_engine_and_sandbox_match_the_published_report() {
    let report = json::parse(include_bytes!("../../../spec/examples/scanner-report.json")).unwrap();
    let payload = report.get("payload").unwrap();
    let expected_engine = payload.get("engine").unwrap();
    let engine = EngineProvenance {
        version: expected_engine
            .get("engine_version")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned(),
        digest: codec::borrow_value::<Digest>("$", expected_engine.get("engine_digest").unwrap())
            .unwrap(),
    };
    assert_eq!(engine_block(&engine).unwrap(), *expected_engine);
    let sandbox = payload.get("controls").unwrap().get("sandbox").unwrap();
    let (descriptor, digest) = sandbox_descriptor().unwrap();
    assert_eq!(&descriptor, sandbox.get("descriptor").unwrap());
    assert_eq!(
        codec::to_value(&digest).unwrap(),
        *sandbox.get("descriptor_digest").unwrap()
    );
}

#[test]
fn failure_projection_preserves_independent_reason_and_error_order() {
    let engine = EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: amiss_wire::digest::hb("test-engine", b"binary"),
    };
    let codes = BTreeSet::from([
        AnalysisErrorCode::RequestUnreadable,
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::InvalidInvocation,
        AnalysisErrorCode::InvalidEvent,
    ]);
    let envelope = invocation_failure_envelope(&engine, &codes).unwrap();
    let payload = envelope.get("payload").unwrap();
    let expected_digest = hj(PAYLOAD_SCHEMA, payload);
    assert_eq!(
        envelope.get("payload_digest").unwrap(),
        &codec::to_value(&expected_digest).unwrap()
    );
    let evaluation = payload.get("evaluation").unwrap();
    assert_eq!(
        evaluation.get("reasons").unwrap(),
        &codec::to_value(&[
            "invalid-invocation",
            "invalid-event",
            "invalid-profile",
            "request-unreadable",
        ])
        .unwrap()
    );
    let actual: Vec<&str> = payload
        .get("errors")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.get("code").unwrap().as_str().unwrap())
        .collect();
    assert_eq!(
        actual,
        [
            "INVALID_EVENT",
            "INVALID_INVOCATION",
            "INVALID_PROFILE",
            "REQUEST_UNREADABLE"
        ]
    );
    assert_eq!(
        payload.get("summary").unwrap(),
        &codec::to_value(&amiss_wire::report::Summary::default()).unwrap()
    );
    assert_eq!(
        payload.get("engine").unwrap(),
        &engine_block(&engine).unwrap()
    );
}

#[test]
fn direct_report_trees_cannot_bypass_the_wire_numeric_profile() {
    for number in [
        json::Value::from(1.5),
        json::Value::from(9_007_199_254_740_992_u64),
    ] {
        let mut report =
            json::parse(include_bytes!("../../../spec/examples/scanner-report.json")).unwrap();
        let payload = report.get_mut("payload").unwrap();
        payload
            .as_object_mut()
            .unwrap()
            .insert("extension".to_owned(), number);
        let digest = hj(PAYLOAD_SCHEMA, payload);
        *report.get_mut("payload_digest").unwrap() = codec::to_value(&digest).unwrap();
        assert_eq!(
            amiss_wire::report::validate_envelope(&report).unwrap_err(),
            amiss_wire::report::ReportDefect::NotAReport
        );
        assert!(codec::from_value::<json::Value>("$", &report).is_err());
    }
}
