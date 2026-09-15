use std::io::Write as _;

use super::model::{ReportEnvelope, ReportEnvelopeSchema, ReportPayload};
use crate::envelope::{Payload as _, write_sealed};
use crate::model::Digest;

/// Writes a complete canonical report, including its newline, through the
/// caller's reserved buffer. A closed fatal report has no detail rows and
/// its shared fields sit in key order, so it streams without spelling, which
/// is what keeps a fatal emission inside the fixed scratch; a report with
/// rows is spelled once.
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
    let payload = &envelope.payload;
    if payload.documents.is_empty()
        && payload.observations.is_empty()
        && payload.findings.is_empty()
    {
        let mut counter = countio::Counter::new(output);
        serde_json::to_writer(&mut counter, envelope)?;
        counter.write_all(b"\n")?;
        counter.flush()?;
        return Ok(u64::try_from(counter.writer_bytes()).unwrap_or(u64::MAX));
    }
    let spelled = payload.spell().map_err(std::io::Error::other)?;
    emit_sealed(&envelope.schema, &spelled, envelope.payload_digest, output)
}

/// Writes a complete canonical report, including its newline, around a payload
/// the pipeline already spelled canonically under `payload_digest`.
///
/// # Errors
/// Returns `InvalidData` when the bytes are not one JSON value, otherwise the
/// first output error; no success count is returned for partial output.
pub fn emit_sealed(
    schema: &ReportEnvelopeSchema,
    payload: &[u8],
    payload_digest: Digest,
    output: &mut impl std::io::Write,
) -> std::io::Result<u64> {
    let mut counter = countio::Counter::new(output);
    write_sealed(schema, payload, payload_digest, &mut counter)?;
    counter.write_all(b"\n")?;
    counter.flush()?;
    Ok(u64::try_from(counter.writer_bytes()).unwrap_or(u64::MAX))
}
