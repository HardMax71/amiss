use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA, ReportDefect,
        model::{ReportEnvelope, ReportStatus},
        validate_envelope,
    },
};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn positional_rows_are_rejected_with_original_and_rebound_digests() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let step = &report.payload.findings[0].policy_trace[0];
    let object = String::from_utf8(serde_json_canonicalizer::to_vec(step).unwrap()).unwrap();
    let sequence =
        serde_json::to_string(&(step.after, step.before, &step.rule_id, step.source)).unwrap();
    let changed = payload.replace(&object, &sequence);
    assert_ne!(payload, changed);
    let wire = std::str::from_utf8(REPORT)
        .unwrap()
        .replace(&payload, &changed);
    let rebound = wire.replace(
        &report.payload_digest.to_string(),
        &hb(PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
    );
    let refused = [wire, rebound].map(|input| {
        assert_eq!(
            serde_json::from_str::<ReportEnvelope>(&input)
                .unwrap()
                .payload,
            report.payload
        );
        validate_envelope(input.as_bytes()).map(drop)
    });
    assert_eq!(refused, [Err(ReportDefect::NotAReport); 2]);
}

#[test]
fn formatting_and_escaped_members_preserve_report_identity() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let expected = validate_envelope(REPORT).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let reordered = format!(
        "{{\"schema\":{},\"payload_digest\":{},\"payload\":{payload}}}",
        serde_json::to_string(&report.schema).unwrap(),
        serde_json::to_string(&report.payload_digest).unwrap(),
    );
    let escaped = reordered.replace(
        "\"compatibility\":\"1\"",
        "\"\\u0063ompatibility\" : \"\\u0031\"",
    );
    assert_ne!(escaped, reordered);
    for input in [
        serde_json::to_string_pretty(&report).unwrap(),
        reordered,
        format!(" \r\n\t{escaped}\n\r "),
    ] {
        assert_eq!(validate_envelope(input.as_bytes()).unwrap(), expected);
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
            validate_envelope(wire.as_bytes()).map(drop),
            Err(ReportDefect::NotAReport),
            "{invalid}"
        );
    }
}

#[test]
fn typed_counts_keep_the_safe_integer_boundary() {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let safe = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER).unwrap();
    for (count, expected) in [(safe, Ok(())), (safe + 1, Err(ReportDefect::NotAReport))] {
        report.payload.summary.findings.warn = count;
        report.payload_digest = hb(
            PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
        );
        let input = serde_json_canonicalizer::to_vec(&report).unwrap();
        assert_eq!(
            serde_json::from_slice::<ReportEnvelope>(&input).unwrap(),
            report
        );
        assert_eq!(validate_envelope(&input).map(drop), expected, "{count}");
    }
}

#[test]
fn payload_digest_precedes_the_semantic_verdict() {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    report.payload.result.status = ReportStatus::Fail;
    assert_eq!(
        validate_envelope(&serde_json_canonicalizer::to_vec(&report).unwrap()).map(drop),
        Err(ReportDefect::DigestMismatch)
    );
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    assert_eq!(
        validate_envelope(&serde_json_canonicalizer::to_vec(&report).unwrap()).map(drop),
        Err(ReportDefect::InvalidResult)
    );
}
