use serde::Deserialize;

use crate::ExitClass;
use crate::digest::{Digest, hj_serde};
use crate::json;

use super::model::{ReportEnvelope, ReportPayload, ReportResult, ReportStatus};
use super::{ENVELOPE_SCHEMA, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ReportDefect};

/// Accepts the active report bytes and returns the typed payload and recorded verdict.
///
/// # Errors
///
/// Refuses oversized or non-strict JSON, invalid report shapes or identities, and
/// typed normalization before checking the payload digest and result tuple.
pub fn validate_envelope(bytes: &[u8]) -> Result<(ReportPayload, Digest, ExitClass), ReportDefect> {
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
    let typed_digest = hj_serde(ENVELOPE_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&envelope, &mut writer)
    })
    .map_err(|_defect| ReportDefect::NotAReport)?;
    let mut input = serde_json::Deserializer::from_slice(bytes);
    input.disable_recursion_limit();
    let input_digest = hj_serde(ENVELOPE_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(
            &serde_transcode::Transcoder::new(&mut input),
            &mut writer,
        )
    })
    .map_err(|_defect| ReportDefect::NotAReport)?;
    if input_digest != typed_digest {
        return Err(ReportDefect::NotAReport);
    }
    let digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&envelope.payload, &mut writer)
    })
    .map_err(|_defect| ReportDefect::NotAReport)?;
    if digest != envelope.payload_digest {
        return Err(ReportDefect::DigestMismatch);
    }
    let verdict = result_verdict(&envelope.payload.result)?;
    Ok((envelope.payload, digest, verdict))
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
