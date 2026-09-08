use std::sync::Arc;

use amiss_wire::digest::hb;
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::report::model::{
    AvailableFeedback, AvailableFeedbackStatus, Feedback, FeedbackItem, ReportEnvelope,
};

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
