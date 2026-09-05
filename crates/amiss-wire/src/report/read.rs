use serde::{Deserialize, de::IgnoredAny};

use crate::ExitClass;
use crate::digest::{Digest, hj_serde};
use crate::json;

use super::model::{ReportEnvelope, ReportPayload, ReportPayloadSchema, ReportStatus};
use super::{COMPATIBILITY, MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, ReportDefect};

#[derive(Deserialize)]
struct PayloadHeader {
    #[serde(rename = "schema")]
    _schema: ReportPayloadSchema,
    compatibility: String,
}

#[derive(Deserialize)]
struct ResultHeader {
    result: ResultTuple,
}

#[derive(Deserialize)]
struct ResultTuple {
    complete: bool,
    status: ReportStatus,
    exit_code: i64,
    #[serde(flatten)]
    _extensions: IgnoredAny,
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
    let verdict = match (result.complete, result.status, result.exit_code) {
        (true, ReportStatus::Pass, 0) => ExitClass::Success,
        (true, ReportStatus::Fail, 1) => ExitClass::BlockingFindings,
        (false, ReportStatus::Incomplete, 2) => ExitClass::Failure,
        (_, _, _) => return Err(ReportDefect::InvalidResult),
    };
    let payload =
        serde_json::from_value(envelope.payload).map_err(|_defect| ReportDefect::NotAReport)?;
    Ok((payload, digest, verdict))
}
