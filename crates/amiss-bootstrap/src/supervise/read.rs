use amiss_wire::digest::hj_serde;
use amiss_wire::json;
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::report::model::{IdentityPayload, ReportEnvelope, Snapshot};
use serde::Deserialize;
use serde_json::Value;

use super::model::{
    BaseEvaluation, CandidateEvaluation, Completion, EnginePayload, EvaluationStatus, FindingCount,
    Findings, Object, PayloadHeader, ResultPayload,
};
use super::{AcceptanceDefect, Expectations, identity};

/// The acceptance law: the wire is exactly `JCS(envelope) || LF`, the
/// payload-only digest recomputes, the engine digest equals the validated
/// binary's, the evaluated identities equal the ones requested, the
/// completeness flag agrees with the exit class, and the finding count equals
/// the findings array length. Text printed before a crash is never
/// interpreted as a result. Success returns the envelope's exit class, so the
/// wrapper can hold the engine process to it.
///
/// # Errors
///
/// The first applicable defect in the order above.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<i64, AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    if !matches!(json::parse(trimmed), Ok(json::Value::Object(_))) {
        return Err(AcceptanceDefect::Shape);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(trimmed);
    // The strict gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    let opaque =
        Value::deserialize(&mut deserializer).map_err(|_defect| AcceptanceDefect::Shape)?;
    if serde_json_canonicalizer::to_vec(&opaque).map_err(|_defect| AcceptanceDefect::Shape)?
        != trimmed
    {
        return Err(AcceptanceDefect::Noncanonical);
    }
    let envelope = serde_json::from_value::<ReportEnvelope<Value>>(opaque)
        .map_err(|_defect| AcceptanceDefect::Shape)?;
    let payload = envelope.payload;
    Object::<PayloadHeader>::deserialize(&payload).map_err(|_defect| AcceptanceDefect::Shape)?;
    let digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&payload, &mut writer)
    })
    .map_err(|_defect| AcceptanceDefect::Shape)?;
    if digest != envelope.payload_digest {
        return Err(AcceptanceDefect::PayloadDigest);
    }
    let engine =
        EnginePayload::deserialize(&payload).map_err(|_defect| AcceptanceDefect::Engine)?;
    if engine.engine.fields.engine_digest != expectations.engine_digest {
        return Err(AcceptanceDefect::Engine);
    }
    let state = IdentityPayload::<Object<EvaluationStatus>>::deserialize(&payload)
        .map_err(|_defect| AcceptanceDefect::Shape)?;
    if state.evaluation.fields.status.is_some() {
        if expectations.sealed.is_some() {
            return Err(AcceptanceDefect::SealedIdentity);
        }
    } else {
        let base = IdentityPayload::<BaseEvaluation>::deserialize(&payload)
            .map_err(|_defect| AcceptanceDefect::BaseIdentity)?;
        if base.evaluation.base.fields.commit_oid != expectations.base_commit {
            return Err(AcceptanceDefect::BaseIdentity);
        }
        IdentityPayload::<CandidateEvaluation<serde::de::IgnoredAny>>::deserialize(&payload)
            .map_err(|_defect| AcceptanceDefect::Shape)?;
        if let Some(expected) = &expectations.candidate_commit {
            let candidate = IdentityPayload::<CandidateEvaluation>::deserialize(&payload)
                .map_err(|_defect| AcceptanceDefect::CandidateIdentity)?;
            if candidate.evaluation.candidate.fields.commit_oid != *expected {
                return Err(AcceptanceDefect::CandidateIdentity);
            }
        } else {
            IdentityPayload::<CandidateEvaluation<Object<Snapshot>>>::deserialize(&payload)
                .map_err(|_defect| AcceptanceDefect::Shape)?;
        }
        if let Some(sealed) = &expectations.sealed {
            identity::accept(&payload, sealed)?;
        }
    }
    let result = ResultPayload::<Completion>::deserialize(&payload)
        .map_err(|_defect| AcceptanceDefect::Shape)?
        .result
        .fields;
    if result.complete != (result.exit_code == 0 || result.exit_code == 1) {
        return Err(AcceptanceDefect::Completeness);
    }
    let count = ResultPayload::<FindingCount>::deserialize(&payload)
        .map_err(|_defect| AcceptanceDefect::Shape)?
        .result
        .fields
        .finding_count;
    let findings = Findings::deserialize(&payload).map_err(|_defect| AcceptanceDefect::Shape)?;
    if i64::try_from(findings.findings.len()).map_err(|_defect| AcceptanceDefect::Shape)? != count {
        return Err(AcceptanceDefect::FindingCount);
    }
    Ok(result.exit_code)
}
