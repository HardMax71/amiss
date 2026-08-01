amiss_fixtures::bounded_memory!();

use std::collections::BTreeSet;

use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{Value, parse};
use amiss_wire::report::{
    AnalysisErrorCode, Disposition, ENGINE_DOMAIN, ENVELOPE_SCHEMA, EngineProvenance, FindingKind,
    PAYLOAD_SCHEMA, invocation_failure_wire,
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
    let feedback = member(payload, "feedback");
    assert_eq!(
        member(feedback, "status"),
        &Value::String("unavailable".to_owned())
    );
    let Value::Object(feedback_members) = feedback else {
        panic!("feedback is not an object");
    };
    assert_eq!(feedback_members.len(), 1);

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
        &Value::String("amiss/scanner".to_owned())
    );
    let Value::Array(adapters) = member(engine_block, "adapters") else {
        panic!("adapters is not an array");
    };
    let ids: Vec<String> = adapters
        .iter()
        .map(|row| strings(&Value::Array(vec![member(row, "adapter_id").clone()])).remove(0))
        .collect();
    assert_eq!(
        ids,
        vec!["asciidoc", "markdown", "mdx", "plain-advisory", "rst"]
    );
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

#[test]
fn removed_references_are_recorded_facts() {
    let kind = FindingKind::ExplicitReferenceRemoved;
    assert_eq!(kind.built_in_disposition(false), Disposition::Record);
    assert_eq!(kind.built_in_disposition(true), Disposition::Record);
    assert_eq!(
        kind.meaning(),
        "a reference that existed in the base is gone from the candidate; the removal is recorded as a fact, never treated as evidence that the edit was wrong"
    );
}

/// Every closed string projection produces a distinct, non-empty name; the
/// class projections repeat legitimately, so they are held to non-emptiness
/// and to naming more than one class across the table.
#[test]
fn the_string_projections_are_populated_and_distinct() {
    use std::collections::BTreeSet;

    use amiss_wire::report::ErrorDetail;

    let error_codes: Vec<AnalysisErrorCode> = AnalysisErrorCode::all().collect();
    let meanings: BTreeSet<&str> = error_codes.iter().map(|code| code.meaning()).collect();
    assert_eq!(meanings.len(), error_codes.len(), "meanings are distinct");
    assert!(meanings.iter().all(|text| !text.is_empty()));
    let phases: BTreeSet<&str> = error_codes
        .iter()
        .map(|code| {
            ErrorDetail {
                code: *code,
                path: None,
                path_bytes: None,
                resource: None,
            }
            .phase()
        })
        .collect();
    assert!(phases.len() > 1, "phases name more than one partition");
    assert!(phases.iter().all(|text| !text.is_empty()));
}

/// The kind and intent tables, held to the same distinctness law.
#[test]
fn the_kind_projections_are_populated_and_distinct() {
    use std::collections::BTreeSet;

    use amiss_wire::report::{Disposition, FindingKind, IntentKind};

    let intents = [
        IntentKind::RepositoryPath,
        IntentKind::SameRepositoryGithub,
        IntentKind::SameRepositoryGitlab,
        IntentKind::SameRepositoryGitea,
        IntentKind::ExternalUrl,
        IntentKind::SiteRoute,
        IntentKind::Label,
        IntentKind::Unsupported,
    ];
    let intent_names: BTreeSet<&str> = intents.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(intent_names.len(), intents.len());
    assert!(intent_names.iter().all(|text| !text.is_empty()));

    let dispositions = [Disposition::Record, Disposition::Warn, Disposition::Fail];
    let disposition_names: BTreeSet<&str> =
        dispositions.iter().map(|value| value.as_str()).collect();
    assert_eq!(disposition_names.len(), dispositions.len());
    assert!(disposition_names.iter().all(|text| !text.is_empty()));

    let kinds: Vec<FindingKind> = FindingKind::all().collect();
    let kind_names: BTreeSet<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(kind_names.len(), kinds.len());
    assert!(kind_names.iter().all(|text| !text.is_empty()));
    let evidence: BTreeSet<&str> = kinds.iter().map(|kind| kind.evidence_class()).collect();
    assert!(evidence.len() > 1 && evidence.iter().all(|text| !text.is_empty()));
    let invariants: BTreeSet<&str> = kinds.iter().map(|kind| kind.invariant_class()).collect();
    assert!(invariants.len() > 1 && invariants.iter().all(|text| !text.is_empty()));
}

/// The fatal serializer's bytes are exactly the canonical wire and a newline,
/// whatever the piece sizes, and its count is the byte count.
#[test]
fn the_fatal_serializer_writes_the_canonical_wire_exactly() {
    use amiss_wire::json::{Value, canonical};
    use amiss_wire::report::{FATAL_SCRATCH_BYTES, FatalSerializer};

    let mut serializer = FatalSerializer::default();
    for length in [
        1,
        FATAL_SCRATCH_BYTES - 1,
        FATAL_SCRATCH_BYTES,
        FATAL_SCRATCH_BYTES + 1,
    ] {
        let members: Vec<(String, Value)> = (0..8)
            .map(|index| {
                (
                    format!("k{index}"),
                    Value::String("v".repeat(length / 4 + 1)),
                )
            })
            .chain(std::iter::once((
                "big".to_owned(),
                Value::String("x".repeat(length)),
            )))
            .collect();
        let envelope = Value::Object(members);
        let mut expected = canonical(&envelope);
        expected.push(b'\n');

        let wire = serializer.wire_bytes(&envelope);
        assert_eq!(wire, expected, "piece length {length}");

        let mut out = Vec::new();
        let written = serializer.emit(&envelope, &mut out).unwrap();
        assert_eq!(out, expected);
        assert_eq!(written, u64::try_from(expected.len()).unwrap());
    }
}

#[test]
fn a_failure_envelope_exists_exactly_when_a_reason_does() {
    use std::collections::BTreeSet;

    use amiss_wire::report::invocation_failure_envelope;

    let mut codes = BTreeSet::new();
    assert!(
        invocation_failure_envelope(&engine(), &codes).is_none(),
        "no code, no envelope"
    );
    codes.insert(AnalysisErrorCode::InvalidInvocation);
    assert!(invocation_failure_envelope(&engine(), &codes).is_some());
}
