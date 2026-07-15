use std::collections::BTreeSet;

use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{Value, parse};
use amiss_wire::report::{
    AnalysisErrorCode, ENGINE_DOMAIN, ENVELOPE_SCHEMA, EngineProvenance, PAYLOAD_SCHEMA,
    invocation_failure_wire,
};

#[expect(clippy::panic, reason = "test navigation helper")]
fn member<'a>(value: &'a Value, key: &str) -> &'a Value {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    members
        .iter()
        .find(|(name, _)| name == key)
        .map_or_else(|| panic!("missing member {key}"), |(_, value)| value)
}

#[expect(clippy::panic, reason = "test navigation helper")]
fn strings(value: &Value) -> Vec<String> {
    let Value::Array(items) = value else {
        panic!("not an array");
    };
    items
        .iter()
        .map(|item| {
            let Value::String(s) = item else {
                panic!("not a string");
            };
            s.clone()
        })
        .collect()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0".to_owned(),
        digest: hb(ENGINE_DOMAIN, b"fake-binary-bytes"),
    }
}

#[test]
fn builds_the_fatal_incomplete_envelope() {
    let codes: BTreeSet<AnalysisErrorCode> = BTreeSet::from([
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::InvalidEvent,
    ]);
    let wire = invocation_failure_wire(&engine(), &codes).unwrap();
    assert_eq!(wire.last(), Some(&b'\n'));
    assert_eq!(invocation_failure_wire(&engine(), &codes).unwrap(), wire);

    let envelope = parse(&wire).unwrap();
    let Value::String(schema) = member(&envelope, "schema") else {
        panic!("schema is not a string");
    };
    assert_eq!(schema, ENVELOPE_SCHEMA);

    let payload = member(&envelope, "payload");
    let Value::String(payload_digest) = member(&envelope, "payload_digest") else {
        panic!("payload_digest is not a string");
    };
    assert_eq!(*payload_digest, hj(PAYLOAD_SCHEMA, payload).to_string());

    let evaluation = member(payload, "evaluation");
    assert_eq!(member(evaluation, "request_digest"), &Value::Null);
    assert_eq!(
        strings(member(evaluation, "reasons")),
        vec!["invalid-event", "invalid-profile"],
        "reasons use enum declaration order"
    );
    assert_eq!(
        strings(member(member(payload, "controls"), "reasons")),
        vec!["not-parsed"]
    );

    let Value::Array(errors) = member(payload, "errors") else {
        panic!("errors is not an array");
    };
    let codes: Vec<String> = errors
        .iter()
        .map(|row| strings(&Value::Array(vec![member(row, "code").clone()])).remove(0))
        .collect();
    assert_eq!(
        codes,
        vec!["INVALID_EVENT", "INVALID_PROFILE"],
        "error rows sort by code bytes"
    );
    for row in errors {
        assert_eq!(
            member(row, "phase"),
            &Value::String("invocation".to_owned())
        );
        assert_eq!(member(row, "path"), &Value::Null);
        assert_eq!(member(row, "resource"), &Value::Null);
        assert_eq!(member(row, "configured_limit"), &Value::Null);
        assert_eq!(member(row, "observed_lower_bound"), &Value::Null);
    }

    let result = member(payload, "result");
    assert_eq!(member(result, "complete"), &Value::Bool(false));
    assert_eq!(
        member(result, "status"),
        &Value::String("incomplete".to_owned())
    );
    assert_eq!(member(result, "exit_code"), &Value::Integer(2));
    assert_eq!(member(result, "finding_count"), &Value::Integer(0));
    assert_eq!(member(result, "error_count"), &Value::Integer(2));

    let summary = member(payload, "summary");
    assert_eq!(member(summary, "counts_complete"), &Value::Bool(false));
    assert_eq!(
        member(member(summary, "documents"), "discovered"),
        &Value::Integer(0)
    );
    for detail in ["documents", "observations", "findings"] {
        assert_eq!(member(payload, detail), &Value::Array(Vec::new()));
    }

    let engine_block = member(payload, "engine");
    assert_eq!(
        member(engine_block, "engine_contract"),
        &Value::String("amiss/scanner-v0".to_owned())
    );
    let Value::Array(adapters) = member(engine_block, "adapters") else {
        panic!("adapters is not an array");
    };
    let ids: Vec<String> = adapters
        .iter()
        .map(|row| strings(&Value::Array(vec![member(row, "adapter_id").clone()])).remove(0))
        .collect();
    assert_eq!(ids, vec!["markdown-v1", "mdx-v1", "plain-advisory-v1"]);
}

#[test]
fn orders_reasons_and_errors_independently() {
    let codes: BTreeSet<AnalysisErrorCode> = BTreeSet::from([
        AnalysisErrorCode::InvalidInvocation,
        AnalysisErrorCode::InvalidEvent,
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::RequestUnreadable,
    ]);
    let wire = invocation_failure_wire(&engine(), &codes).unwrap();
    let envelope = parse(&wire).unwrap();
    let payload = member(&envelope, "payload");
    assert_eq!(
        strings(member(member(payload, "evaluation"), "reasons")),
        vec![
            "invalid-invocation",
            "invalid-event",
            "invalid-profile",
            "request-unreadable"
        ]
    );
    let Value::Array(errors) = member(payload, "errors") else {
        panic!("errors is not an array");
    };
    let codes: Vec<String> = errors
        .iter()
        .map(|row| strings(&Value::Array(vec![member(row, "code").clone()])).remove(0))
        .collect();
    assert_eq!(
        codes,
        vec![
            "INVALID_EVENT",
            "INVALID_INVOCATION",
            "INVALID_PROFILE",
            "REQUEST_UNREADABLE"
        ]
    );
}

#[test]
fn refuses_inputs_outside_the_invocation_phase() {
    assert!(invocation_failure_wire(&engine(), &BTreeSet::new()).is_none());
    let git: BTreeSet<AnalysisErrorCode> = BTreeSet::from([AnalysisErrorCode::GitObjectMissing]);
    assert!(invocation_failure_wire(&engine(), &git).is_none());
}
