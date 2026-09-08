use amiss_wire::digest::hb;
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::report::model::{
    AvailableFeedback, AvailableFeedbackStatus, Feedback, FeedbackItem, ReportEnvelope,
};

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
