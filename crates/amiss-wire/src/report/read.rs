use serde::Deserialize;

use crate::ExitClass;
use crate::digest::{Digest, hj_serde};
use crate::json;

use super::model::{
    ReportEnvelope, ReportPayload, ReportPayloadSchema, ReportResult, ReportStatus,
};
use super::{COMPATIBILITY, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ReportDefect};

#[derive(Deserialize)]
struct PayloadHeader {
    #[serde(rename = "schema")]
    _schema: ReportPayloadSchema,
    compatibility: String,
}

#[derive(Deserialize)]
struct ResultHeader {
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    result: ReportResult,
}

/// Accepts the active report bytes and returns the typed payload and recorded verdict.
/// Additive fields remain digest-bound within the supported compatibility.
///
/// # Errors
///
/// Refuses oversized or non-strict JSON, unsupported report identities, and invalid
/// payload digests, result tuples, or known fields.
pub fn validate_envelope(bytes: &[u8]) -> Result<(ReportPayload, Digest, ExitClass), ReportDefect> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES
        || !matches!(json::parse(bytes), Ok(json::Value::Object(_)))
    {
        return Err(ReportDefect::NotAReport);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    // The strict gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    let envelope: ReportEnvelope<serde_json::Value> =
        ReportEnvelope::deserialize(&mut deserializer)
            .map_err(|_defect| ReportDefect::NotAReport)?;
    if !envelope.payload.is_object() {
        return Err(ReportDefect::NotAReport);
    }
    let header = PayloadHeader::deserialize(&envelope.payload)
        .map_err(|_defect| ReportDefect::NotAReport)?;
    if header.compatibility != COMPATIBILITY {
        return Err(ReportDefect::UnsupportedCompatibility);
    }
    let digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&envelope.payload, &mut writer)
    })
    .map_err(|_defect| ReportDefect::NotAReport)?;
    if digest != envelope.payload_digest {
        return Err(ReportDefect::DigestMismatch);
    }
    let ResultHeader { result } = ResultHeader::deserialize(&envelope.payload)
        .map_err(|_defect| ReportDefect::InvalidResult)?;
    let verdict = result_verdict(&result)?;
    let payload =
        serde_json::from_value(envelope.payload).map_err(|_defect| ReportDefect::NotAReport)?;
    Ok((payload, digest, verdict))
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
