use std::collections::BTreeSet;

use amiss_wire::digest::hb;
use amiss_wire::report::model::{Controls, Evaluation, ReportEnvelope};
use amiss_wire::report::{
    AnalysisErrorCode, Disposition, ENGINE_DOMAIN, ENVELOPE_SCHEMA, EngineProvenance, FindingKind,
    FixKind, PAYLOAD_SCHEMA, invocation_failure_wire,
};

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0".to_owned(),
        digest: hb(ENGINE_DOMAIN, b"fake-binary-bytes"),
    }
}

#[test]
fn builds_the_fatal_incomplete_envelope() {
    let (descriptor, descriptor_digest) = amiss_wire::report::sandbox_descriptor().unwrap();
    let descriptor_bytes = serde_json_canonicalizer::to_vec(&descriptor).unwrap();
    assert_eq!(serde_json::to_vec(&descriptor).unwrap(), descriptor_bytes);
    assert_eq!(
        descriptor_digest,
        hb(amiss_wire::report::SANDBOX_SCHEMA, &descriptor_bytes)
    );
    let codes: BTreeSet<AnalysisErrorCode> = BTreeSet::from([
        AnalysisErrorCode::InvalidProfile,
        AnalysisErrorCode::InvalidEvent,
    ]);
    let wire = invocation_failure_wire(&engine(), &codes).unwrap().unwrap();
    assert_eq!(wire.last(), Some(&b'\n'));
    assert_eq!(
        invocation_failure_wire(&engine(), &codes).unwrap().unwrap(),
        wire
    );

    let envelope: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    assert_eq!(
        serde_json_canonicalizer::to_vec(&envelope).unwrap(),
        amiss_fixtures::canonical_json(&wire).unwrap()
    );
    assert_eq!(envelope.schema.to_string(), ENVELOPE_SCHEMA);

    let payload = &envelope.payload;
    let payload_bytes = serde_json_canonicalizer::to_vec(payload).unwrap();
    assert_eq!(envelope.payload_digest, hb(PAYLOAD_SCHEMA, &payload_bytes));

    let Evaluation::Unavailable(evaluation) = &payload.evaluation else {
        panic!("an invocation refusal has no resolved evaluation");
    };
    assert!(evaluation.request_digest.is_none());
    assert_eq!(
        evaluation
            .reasons
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>(),
        ["invalid-event", "invalid-profile"],
        "reasons use enum declaration order"
    );
    let Controls::Unavailable(controls) = &payload.controls else {
        panic!("an invocation refusal has no resolved controls");
    };
    assert!(controls.request_digest.is_none());
    assert_eq!(
        controls
            .reasons
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>(),
        ["not-parsed"]
    );
    assert_eq!(
        serde_json::to_string(&payload.feedback).unwrap(),
        r#"{"status":"unavailable"}"#
    );

    let codes: Vec<_> = payload.errors.iter().map(|row| row.code.as_ref()).collect();
    assert_eq!(
        codes,
        ["INVALID_EVENT", "INVALID_PROFILE"],
        "error rows sort by code bytes"
    );
    for row in &payload.errors {
        assert_eq!(row.phase.as_ref(), "invocation");
        assert!(row.path.is_none());
        assert!(row.path_bytes_hex.is_none());
        assert!(row.resource.is_none());
        assert!(row.configured_limit.is_none());
        assert!(row.observed_lower_bound.is_none());
    }

    let result = &payload.result;
    assert!(!result.complete);
    assert_eq!(result.status.as_ref(), "incomplete");
    assert_eq!(result.exit_code, 2);
    assert_eq!(result.finding_count, 0);
    assert_eq!(result.error_count, 2);

    assert!(!payload.summary.counts_complete);
    assert_eq!(payload.summary.documents.discovered, 0);
    assert!(payload.documents.is_empty());
    assert!(payload.observations.is_empty());
    assert!(payload.findings.is_empty());

    assert_eq!(payload.engine.engine_contract.to_string(), "amiss/scanner");
    let ids: Vec<_> = payload
        .engine
        .adapters
        .iter()
        .map(|row| row.adapter_id.as_ref())
        .collect();
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
    let envelope: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let Evaluation::Unavailable(evaluation) = &envelope.payload.evaluation else {
        panic!("an invocation refusal has no resolved evaluation");
    };
    assert_eq!(
        evaluation
            .reasons
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>(),
        [
            "invalid-invocation",
            "invalid-event",
            "invalid-profile",
            "request-unreadable"
        ]
    );
    let codes: Vec<_> = envelope
        .payload
        .errors
        .iter()
        .map(|row| row.code.as_ref())
        .collect();
    assert_eq!(
        codes,
        [
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
        hb(
            "amiss/test-error-routes",
            &serde_json_canonicalizer::to_vec(&rows).unwrap()
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
