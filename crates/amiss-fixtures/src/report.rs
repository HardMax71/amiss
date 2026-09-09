use std::sync::Arc;

use amiss_wire::digest::{hb, hj_serde};
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::report::model::{
    AvailableFeedback, AvailableFeedbackStatus, Feedback, FeedbackItem, ReportEnvelope,
};

/// Rebinds a typed fixture's payload and emits canonical JSON with one trailing LF.
///
/// # Errors
/// Returns the library error if the report cannot be serialized.
pub fn report_bytes(mut report: ReportEnvelope) -> serde_json::Result<Vec<u8>> {
    report.payload_digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })?;
    let mut wire = serde_json_canonicalizer::to_vec(&report)?;
    wire.push(b'\n');
    Ok(wire)
}

/// Replaces one fixture fragment and rebinds the report's payload digest.
///
/// # Errors
/// Returns the library error for malformed JSON or serialization failure.
///
/// # Panics
/// The fragment, payload or digest is absent or ambiguous, or the replacement changes nothing.
pub fn corrupt(
    report: &ReportEnvelope,
    original: &str,
    replacement: &str,
) -> serde_json::Result<Vec<u8>> {
    let payload = serde_json_canonicalizer::to_string(&report.payload)?;
    assert_eq!(payload.matches(original).count(), 1, "{original}");
    let changed = payload.replacen(original, replacement, 1);
    assert_ne!(changed, payload);
    let mut source = serde_json::Deserializer::from_str(&changed);
    let changed =
        serde_json_canonicalizer::to_string(&serde_transcode::Transcoder::new(&mut source))?;
    source.end()?;
    let mut wire = serde_json_canonicalizer::to_string(report)?;
    let digest = report.payload_digest.to_string();
    assert_eq!(wire.matches(&payload).count(), 1);
    assert_eq!(wire.matches(&digest).count(), 1);
    wire = wire.replacen(&payload, &changed, 1).replacen(
        &digest,
        &hb(PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
        1,
    );
    wire.push('\n');
    Ok(wire.into_bytes())
}

/// Captures a complete fixture report without changing its source bytes.
///
/// # Errors
/// Refuses malformed envelopes, payload digests and verdicts.
pub fn captured_report(
    bytes: Vec<u8>,
) -> Result<Arc<amiss_wire::report::CapturedReport>, amiss_wire::report::ReportDefect> {
    let (envelope, _verdict) = amiss_wire::report::validate_envelope(&bytes)?;
    Ok(Arc::new(amiss_wire::report::CapturedReport {
        bytes,
        envelope,
    }))
}

/// Builds a complete, digest-true report with the supplied feedback.
///
/// # Errors
/// Refuses an unreadable example or unencodable report.
pub fn feedback_report(
    existing_count: u64,
    items: Vec<FeedbackItem>,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut report: ReportEnvelope = serde_json::from_slice(crate::SCANNER_REPORT)?;
    report.payload.feedback = Feedback::Available(AvailableFeedback {
        existing_count,
        items,
        status: AvailableFeedbackStatus::Available,
    });
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload)?,
    );
    serde_json_canonicalizer::to_vec(&report)
}
