use std::io::Write as _;

use super::model::{ReportEnvelope, ReportPayload};

/// Writes a complete canonical report, including its newline, through the caller's reserved buffer.
/// Closed fatal reports have no detail rows and their shared fields are already in canonical order.
///
/// # Errors
/// Returns the first serialization or output error; no success count is returned for partial output.
pub fn emit_report<P, R, M, E>(
    envelope: &ReportEnvelope<ReportPayload<P, R, M, E>>,
    output: &mut impl std::io::Write,
) -> std::io::Result<u64>
where
    ReportPayload<P, R, M, E>: serde::Serialize,
{
    let mut counter = countio::Counter::new(output);
    let payload = &envelope.payload;
    if payload.documents.is_empty()
        && payload.observations.is_empty()
        && payload.findings.is_empty()
    {
        serde_json::to_writer(&mut counter, envelope)?;
    } else {
        serde_json_canonicalizer::to_writer(envelope, &mut counter)?;
    }
    counter.write_all(b"\n")?;
    counter.flush()?;
    Ok(u64::try_from(counter.writer_bytes()).unwrap_or(u64::MAX))
}
