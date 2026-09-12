use amiss_wire::report::{
    AnalysisErrorCode, Disposition, ENGINE_DOMAIN, ENVELOPE_SCHEMA, EngineProvenance, FindingKind,
    FixKind, PAYLOAD_SCHEMA, invocation_failure_wire,
};
use serde_json::Value;
use sha2::Digest as _;
use std::collections::BTreeSet;

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0".to_owned(),
        digest: amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(ENGINE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(b"fake-binary-bytes")
                .finalize()
                .0,
        ),
    }
}

#[test]
fn builds_the_fatal_incomplete_envelope() {
    let codes = BTreeSet::from([
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::InvalidEvent,
    ]);
    let wire = invocation_failure_wire(&engine(), &codes).unwrap().unwrap();
    assert_eq!(wire.last(), Some(&b'\n'));
    let envelope: Value = serde_json::from_slice(&wire).unwrap();
    for (path, expected) in [
        ("/schema", serde_json::json!(ENVELOPE_SCHEMA)),
        ("/payload/evaluation/request_digest", Value::Null),
        (
            "/payload/evaluation/reasons",
            serde_json::json!(["invalid-event", "invalid-profile"]),
        ),
        (
            "/payload/controls/reasons",
            serde_json::json!(["not-parsed"]),
        ),
        (
            "/payload/feedback",
            serde_json::json!({"status": "unavailable"}),
        ),
        ("/payload/result/complete", Value::Bool(false)),
        ("/payload/result/status", serde_json::json!("incomplete")),
        ("/payload/result/exit_code", serde_json::json!(2)),
        ("/payload/result/finding_count", serde_json::json!(0)),
        ("/payload/result/error_count", serde_json::json!(2)),
        ("/payload/summary/counts_complete", Value::Bool(false)),
        (
            "/payload/summary/documents/discovered",
            serde_json::json!(0),
        ),
        (
            "/payload/engine/engine_contract",
            serde_json::json!("amiss/scanner"),
        ),
    ] {
        assert_eq!(envelope.pointer(path), Some(&expected), "{path}");
    }
    let payload = &envelope["payload"];
    let payload_bytes = serde_json_canonicalizer::to_vec(payload).unwrap();
    assert_eq!(
        envelope["payload_digest"],
        serde_json::json!(amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(payload_bytes)
                .finalize()
                .0
        ))
    );
    let errors = payload["errors"].as_array().unwrap();
    assert_eq!(
        errors
            .iter()
            .map(|row| row["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["INVALID_EVENT", "INVALID_PROFILE"]
    );
    for row in errors {
        assert_eq!(row["phase"], "invocation");
        for field in [
            "path",
            "resource",
            "configured_limit",
            "observed_lower_bound",
        ] {
            assert_eq!(row.get(field), Some(&Value::Null), "{field}");
        }
    }
    for detail in ["documents", "observations", "findings"] {
        assert_eq!(payload[detail], serde_json::json!([]));
    }
    let ids = payload["engine"]["adapters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["adapter_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["asciidoc", "markdown", "mdx", "plain-advisory", "rst"]
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
    let wire = invocation_failure_wire(&engine(), &codes).unwrap().unwrap();
    let envelope = serde_json::from_slice::<Value>(&wire).unwrap();
    let payload = envelope.get("payload").expect("fixture member exists");
    assert_eq!(
        <Vec<String> as serde::Deserialize>::deserialize(
            ((payload).get("evaluation").expect("fixture member exists"))
                .get("reasons")
                .expect("fixture member exists")
        )
        .unwrap(),
        vec![
            "invalid-invocation",
            "invalid-event",
            "invalid-profile",
            "request-unreadable"
        ]
    );
    let Value::Array(errors) = (payload).get("errors").expect("fixture member exists") else {
        panic!("errors is not an array");
    };
    let codes: Vec<String> = errors
        .iter()
        .map(|row| {
            <Vec<String> as serde::Deserialize>::deserialize(&Value::Array(vec![
                (row).get("code").expect("fixture member exists").clone(),
            ]))
            .unwrap()
            .remove(0)
        })
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
    assert!(
        invocation_failure_wire(&engine(), &BTreeSet::new())
            .unwrap()
            .is_none()
    );
    let git: BTreeSet<AnalysisErrorCode> = BTreeSet::from([AnalysisErrorCode::GitObjectMissing]);
    assert!(invocation_failure_wire(&engine(), &git).unwrap().is_none());
}

#[test]
fn error_routes_preserve_the_canonical_wire() {
    use amiss_wire::controls::ResourceName;
    use amiss_wire::report::{ErrorDetail, error_row};

    let ordinary = AnalysisErrorCode::all().map(|code| ErrorDetail {
        code,
        path: None,
        path_bytes: None,
        resource: None,
    });
    let resources = ResourceName::all().map(|name| ErrorDetail {
        code: AnalysisErrorCode::ResourceLimitExceeded,
        path: None,
        path_bytes: None,
        resource: Some((name, 1, 2)),
    });
    let rows: Vec<_> = ordinary
        .chain(resources)
        .map(|detail| {
            let row = error_row(&detail);
            assert_eq!(row.code, detail.code);
            assert_eq!(row.description, detail.code.meaning());
            row
        })
        .collect();
    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/test-error-routes")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&rows).unwrap())
                .finalize()
                .0
        )
        .to_string(),
        "sha256:25a99d8043b027e8da184a3a0458e984e2180da5399bc96957e9a9d86c590274",
    );
}

#[test]
fn removed_references_are_recorded_facts() {
    let kind = FindingKind::ExplicitReferenceRemoved;
    assert_eq!(
        kind.built_in_disposition(amiss_wire::controls::Profile::Observe),
        Disposition::Record
    );
    assert_eq!(
        kind.built_in_disposition(amiss_wire::controls::Profile::Enforce),
        Disposition::Record
    );
    assert_eq!(
        kind.meaning(),
        "a reference that existed in the base is gone from the candidate; the removal is recorded as a fact, never treated as evidence that the edit was wrong"
    );
}

#[test]
fn the_error_meanings_are_populated_and_distinct() {
    use std::collections::BTreeSet;

    let error_codes: Vec<AnalysisErrorCode> = AnalysisErrorCode::all().collect();
    let meanings: BTreeSet<&str> = error_codes.iter().map(|code| code.meaning()).collect();
    assert_eq!(meanings.len(), error_codes.len(), "meanings are distinct");
    assert!(meanings.iter().all(|text| !text.is_empty()));
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
        IntentKind::SameRepositoryBitbucketCloud,
        IntentKind::SameRepositoryBitbucketDataCenter,
        IntentKind::ExternalUrl,
        IntentKind::SiteRoute,
        IntentKind::Label,
        IntentKind::Unsupported,
    ];
    let intent_names: BTreeSet<&str> = intents.iter().map(AsRef::as_ref).collect();
    assert_eq!(intent_names.len(), intents.len());
    assert!(intent_names.iter().all(|text| !text.is_empty()));

    let dispositions = [Disposition::Record, Disposition::Warn, Disposition::Fail];
    let disposition_names: BTreeSet<&str> = dispositions.iter().map(AsRef::as_ref).collect();
    assert_eq!(disposition_names.len(), dispositions.len());
    assert!(disposition_names.iter().all(|text| !text.is_empty()));

    let kinds: Vec<FindingKind> = FindingKind::all().collect();
    let kind_names: BTreeSet<&str> = kinds.iter().map(AsRef::as_ref).collect();
    assert_eq!(kind_names.len(), kinds.len());
    assert!(kind_names.iter().all(|text| !text.is_empty()));
    let evidence: BTreeSet<&str> = kinds
        .iter()
        .map(|kind| kind.metadata().evidence_class.as_ref())
        .collect();
    assert!(evidence.len() > 1 && evidence.iter().all(|text| !text.is_empty()));
    let invariants: BTreeSet<&str> = kinds
        .iter()
        .map(|kind| kind.metadata().invariant_class.as_ref())
        .collect();
    assert!(invariants.len() > 1 && invariants.iter().all(|text| !text.is_empty()));
}

#[test]
fn a_failure_envelope_exists_exactly_when_a_reason_does() {
    use std::collections::BTreeSet;

    use amiss_wire::report::invocation_failure_envelope;

    let mut codes = BTreeSet::new();
    assert!(
        invocation_failure_envelope(&engine(), &codes)
            .unwrap()
            .is_none(),
        "no code, no envelope"
    );
    codes.insert(AnalysisErrorCode::InvalidInvocation);
    assert!(
        invocation_failure_envelope(&engine(), &codes)
            .unwrap()
            .is_some()
    );
}

/// The fix vocabulary answers like the other fixed-sentence tables: every
/// rewrite names itself, and no two share a sentence.
#[test]
fn every_fix_kind_states_its_own_sentence() {
    let sentences = [
        FixKind::ClaimValueRewrite,
        FixKind::AnchorRespelling,
        FixKind::PathRespelling,
    ]
    .map(FixKind::meaning);
    assert!(sentences.iter().all(|sentence| !sentence.is_empty()));
    let unique: BTreeSet<&str> = sentences.iter().copied().collect();
    assert_eq!(unique.len(), sentences.len(), "{sentences:?}");
}

#[test]
fn report_emission_preserves_bytes_and_propagates_short_writes() {
    use amiss_wire::report::model::ReportEnvelope;
    use amiss_wire::report::{FATAL_SCRATCH_BYTES, emit_report};
    use std::io::{BufWriter, Cursor, ErrorKind};

    let codes = BTreeSet::from([AnalysisErrorCode::InvalidInvocation]);
    let refusal = invocation_failure_wire(&engine(), &codes).unwrap().unwrap();
    let normal: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");
    for bytes in [normal, refusal.as_slice()] {
        let envelope: ReportEnvelope = serde_json::from_slice(bytes).unwrap();
        let mut expected = serde_json_canonicalizer::to_vec(&envelope).unwrap();
        expected.push(b'\n');
        for capacity in [0, 1, FATAL_SCRATCH_BYTES] {
            let mut out = BufWriter::with_capacity(capacity, Vec::new());
            let written = emit_report(&envelope, &mut out).unwrap();
            assert_eq!(
                out.into_inner().unwrap(),
                expected,
                "buffer capacity {capacity}"
            );
            assert_eq!(written, u64::try_from(expected.len()).unwrap());
            let mut short = vec![0; expected.len() - 1];
            let mut output = BufWriter::with_capacity(capacity, Cursor::new(short.as_mut_slice()));
            assert_eq!(
                emit_report(&envelope, &mut output).unwrap_err().kind(),
                ErrorKind::WriteZero,
                "buffer capacity {capacity}"
            );
        }
    }
}
