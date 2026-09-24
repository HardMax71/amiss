use amiss_wire::envelope::{Payload as _, document_digest, sealed_digest};
use amiss_wire::report::{
    PAYLOAD_SCHEMA, ReportDefect, emit_report, emit_sealed,
    model::{
        DocumentStatus, ReportEnvelope, ReportPayload, ReportResolution, ReportStatus, Sides,
        UnsupportedReason, UnsupportedSemanticsResolution,
    },
};
use sha2::Digest as _;

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn normalized_fields_are_rejected_with_original_and_rebound_digests() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let step = &report.payload.findings[0].policy_trace[0];
    let object = String::from_utf8(serde_json_canonicalizer::to_vec(step).unwrap()).unwrap();
    let sequence =
        serde_json::to_string(&(step.after, step.before, &step.rule_id, step.source)).unwrap();
    for changed in [
        payload.replace(&object, &sequence),
        payload.replace("\"evaluation_instant\":null,", ""),
    ] {
        assert_ne!(payload, changed);
        let wire = std::str::from_utf8(REPORT)
            .unwrap()
            .replace(&payload, &changed);
        let rebound = wire.replace(
            &report.payload_digest.to_string(),
            &amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(changed.as_bytes())
                    .finalize()
                    .0,
            )
            .to_string(),
        );
        let refused = [wire, rebound].map(|input| {
            assert_eq!(
                serde_json::from_str::<ReportEnvelope>(&input)
                    .unwrap()
                    .payload,
                report.payload
            );
            <ReportPayload>::parse(input.as_bytes()).map(drop)
        });
        assert_eq!(refused, [Err(ReportDefect::Noncanonical); 2]);
    }
}

#[test]
fn formatting_and_escaped_members_preserve_report_identity() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let expected = <ReportPayload>::parse(REPORT).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let reordered = format!(
        "{{\"schema\":{},\"payload_digest\":{},\"payload\":{payload}}}",
        serde_json::to_string(&report.schema).unwrap(),
        serde_json::to_string(&report.payload_digest).unwrap(),
    );
    let escaped = reordered.replace(
        "\"compatibility\":\"3\"",
        "\"\\u0063ompatibility\" : \"\\u0033\"",
    );
    assert_ne!(escaped, reordered);
    for input in [
        serde_json::to_string_pretty(&report).unwrap(),
        reordered,
        format!(" \r\n\t{escaped}\n\r "),
    ] {
        assert_eq!(<ReportPayload>::parse(input.as_bytes()).unwrap(), expected);
    }
}

#[test]
fn known_fields_cannot_hide_non_strict_json_tokens() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let counts = &report.payload.summary.findings;
    let object = String::from_utf8(serde_json_canonicalizer::to_vec(counts).unwrap()).unwrap();
    let count = format!("\"warn\":{}", counts.warn);
    for invalid in [
        "\"warn\":-0",
        "\"warn\":0.0",
        "\"warn\":0e0",
        "\"warn\":9007199254740992",
        "\"warn\":0,\"warn\":0",
        "\"warn\":0,\"\\u0077arn\":0",
    ] {
        let changed = object.replace(&count, invalid);
        assert_ne!(object, changed);
        let wire = std::str::from_utf8(REPORT)
            .unwrap()
            .replace(&object, &changed);
        assert_eq!(
            <ReportPayload>::parse(wire.as_bytes()).map(drop),
            Err(ReportDefect::NotAReport),
            "{invalid}"
        );
    }
}

#[test]
fn typed_counts_keep_the_safe_integer_boundary() {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let safe = u64::try_from(js_int::MAX_SAFE_INT).unwrap();
    assert_eq!(safe, js_int::MAX_SAFE_UINT);
    report.payload.summary.findings.warn = safe;
    report.payload_digest = document_digest(PAYLOAD_SCHEMA, &report.payload).unwrap();
    let input = serde_json_canonicalizer::to_vec(&report).unwrap();
    assert_eq!(
        serde_json::from_slice::<ReportEnvelope>(&input).unwrap(),
        report
    );
    assert!(<ReportPayload>::parse(&input).is_ok());

    let invalid = String::from_utf8(input)
        .unwrap()
        .replace(&safe.to_string(), &(safe + 1).to_string());
    assert!(serde_json::from_str::<ReportEnvelope>(&invalid).is_err());
    assert_eq!(
        <ReportPayload>::parse(invalid.as_bytes()).map(drop),
        Err(ReportDefect::NotAReport)
    );
    report.payload.summary.findings.warn = safe + 1;
    assert!(serde_json::to_vec(&report).is_err());
    assert!(serde_json_canonicalizer::to_vec(&report).is_err());
}

#[test]
fn payload_digest_precedes_the_semantic_verdict() {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    report.payload.result.status = ReportStatus::Fail;
    assert_eq!(
        <ReportPayload>::parse(&serde_json_canonicalizer::to_vec(&report).unwrap()).map(drop),
        Err(ReportDefect::DigestMismatch)
    );
    report.payload_digest = document_digest(PAYLOAD_SCHEMA, &report.payload).unwrap();
    assert_eq!(
        <ReportPayload>::parse(&serde_json_canonicalizer::to_vec(&report).unwrap()).map(drop),
        Err(ReportDefect::InvalidResult)
    );
}

#[test]
fn an_unfamiliar_reason_survives_a_read_and_a_rewrite() {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let document: UnsupportedReason = "generated-page".parse().unwrap();
    assert_eq!(
        document,
        UnsupportedReason::Unrecognized("generated-page".to_owned())
    );
    for side in report
        .payload
        .documents
        .iter_mut()
        .flat_map(|row| [row.base.as_mut(), row.candidate.as_mut()])
        .flatten()
    {
        side.status = DocumentStatus::Unsupported;
        side.unsupported_reason = Some(document.clone());
    }
    let semantics = UnsupportedSemanticsResolution {
        reason: "unmodelled-route".parse().unwrap(),
        target: None,
    };
    let mut carried = 0_u32;
    for comparison in &mut report.payload.observations {
        let side = match &mut comparison.sides {
            Sides::Same(occurrence) => Some(occurrence.as_mut()),
            Sides::Each(pair) => pair.base.as_mut().or(pair.candidate.as_mut()),
        };
        if let Some(occurrence) = side {
            occurrence.resolution = ReportResolution::UnsupportedSemantics(semantics.clone());
            carried += 1;
        }
    }
    assert!(carried > 0);

    let spelled = report.payload.spell().unwrap();
    report.payload_digest = sealed_digest(PAYLOAD_SCHEMA, &spelled);
    let mut wire = Vec::new();
    emit_sealed(&report.schema, &spelled, report.payload_digest, &mut wire).unwrap();
    let text = std::str::from_utf8(&wire).unwrap();
    assert!(text.contains("\"unsupported_reason\":\"generated-page\""));
    assert!(text.contains("\"reason\":\"unmodelled-route\""));

    let read = <ReportPayload>::parse(&wire).unwrap();
    let mut rewritten = Vec::new();
    emit_report(&read, &mut rewritten).unwrap();
    assert_eq!(rewritten, wire);
}
