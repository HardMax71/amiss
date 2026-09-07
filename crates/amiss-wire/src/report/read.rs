use serde::Deserialize;

use crate::ExitClass;
use crate::digest::{hj_serde, verified_json_digest};
use crate::json;

use super::model::{ReportEnvelope, ReportResult, ReportStatus};
use super::{ENVELOPE_SCHEMA, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ReportDefect};

/// Accepts the active report bytes and returns the complete typed envelope and verdict.
///
/// # Errors
///
/// Refuses oversized or non-strict JSON, invalid report shapes or identities, and
/// typed normalization before checking the payload digest and result tuple.
pub fn validate_envelope(bytes: &[u8]) -> Result<(ReportEnvelope, ExitClass), ReportDefect> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES
        || !matches!(json::parse(bytes), Ok(json::Value::Object(_)))
    {
        return Err(ReportDefect::NotAReport);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    // The strict gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    let envelope: ReportEnvelope = ReportEnvelope::deserialize(&mut deserializer)
        .map_err(|_defect| ReportDefect::NotAReport)?;
    verified_json_digest(ENVELOPE_SCHEMA, bytes, &envelope)
        .map_err(|_defect| ReportDefect::NotAReport)?;
    let verdict = validate_report(&envelope)?;
    Ok((envelope, verdict))
}

pub(crate) fn validate_report(envelope: &ReportEnvelope) -> Result<ExitClass, ReportDefect> {
    let digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&envelope.payload, &mut writer)
    })
    .map_err(|_defect| ReportDefect::NotAReport)?;
    if digest != envelope.payload_digest {
        return Err(ReportDefect::DigestMismatch);
    }
    result_verdict(&envelope.payload.result)
}

/// Checks the recorded completeness, status and exit code as one verdict.
///
/// # Errors
/// Refuses inconsistent or unsupported result tuples.
pub fn result_verdict(result: &ReportResult) -> Result<ExitClass, ReportDefect> {
    match (result.complete, result.status, result.exit_code) {
        (true, ReportStatus::Pass, 0) => Ok(ExitClass::Success),
        (true, ReportStatus::Fail, 1) => Ok(ExitClass::BlockingFindings),
        (false, ReportStatus::Incomplete, 2) => Ok(ExitClass::Failure),
        (_, _, _) => Err(ReportDefect::InvalidResult),
    }
}
