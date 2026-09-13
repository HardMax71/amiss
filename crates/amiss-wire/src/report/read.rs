use crate::envelope::{document_digest, transcoded_digest};

use crate::ExitClass;
use crate::model::Digest;

use super::model::{ReportEnvelope, ReportPayload, ReportResult, ReportStatus};
use super::{ENVELOPE_SCHEMA, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ReportDefect};

/// Accepts the active report bytes and returns the typed payload and recorded verdict.
///
/// # Errors
///
/// Refuses oversized or malformed JSON, invalid report shapes or identities, and
/// typed normalization before checking the payload digest and result tuple.
pub fn validate_envelope(bytes: &[u8]) -> Result<(ReportPayload, Digest, ExitClass), ReportDefect> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        return Err(ReportDefect::NotAReport);
    }
    let envelope: ReportEnvelope =
        serde_json::from_slice(bytes).map_err(|_defect| ReportDefect::NotAReport)?;
    let typed_digest =
        document_digest(ENVELOPE_SCHEMA, &envelope).ok_or(ReportDefect::NotAReport)?;
    let input_digest = transcoded_digest(ENVELOPE_SCHEMA, bytes).ok_or(ReportDefect::NotAReport)?;
    if input_digest != typed_digest {
        return Err(ReportDefect::NotAReport);
    }
    let digest =
        document_digest(PAYLOAD_SCHEMA, &envelope.payload).ok_or(ReportDefect::NotAReport)?;
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
