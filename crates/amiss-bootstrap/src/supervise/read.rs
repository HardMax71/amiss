use amiss_wire::report::model::{BaseSnapshot, Evaluation, ReportEnvelope, Snapshot};
use amiss_wire::report::{PAYLOAD_SCHEMA, result_verdict};
use amiss_wire::requests::CandidateSnapshot;
use serde::Deserialize;
use sha2::Digest as _;

use super::{AcceptanceDefect, Expectations, identity};

/// Checks a full report's shape, canonical bytes and bindings, returning its exit class.
///
/// # Errors
///
/// Requires LF framing and a closed typed envelope, then checks canonical bytes,
/// payload digest, engine/base/candidate identities, sealed controls, verdict and
/// finding count in that order.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<i64, AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    if amiss_wire::de::JsonProfile::validate(trimmed).is_err() {
        return Err(AcceptanceDefect::Shape);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(trimmed);
    // The strict gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    let envelope: ReportEnvelope = <ReportEnvelope as Deserialize>::deserialize(&mut deserializer)
        .map_err(|_defect| AcceptanceDefect::Shape)?;
    if serde_json_canonicalizer::to_vec(&envelope).map_err(|_defect| AcceptanceDefect::Shape)?
        != trimmed
    {
        return Err(AcceptanceDefect::Noncanonical);
    }
    let payload = envelope.payload;
    let digest = {
        let mut writer = digest_io::IoWrapper(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA).chain_update([0_u8]),
        );
        serde_json_canonicalizer::to_writer(&payload, &mut writer)
            .map(|()| amiss_wire::model::Digest::from(writer.0.finalize().0))
    }
    .map_err(|_defect| AcceptanceDefect::Shape)?;
    if digest != envelope.payload_digest {
        return Err(AcceptanceDefect::PayloadDigest);
    }
    if payload.engine.engine_digest != expectations.engine_digest {
        return Err(AcceptanceDefect::Engine);
    }
    match &payload.evaluation {
        Evaluation::Unavailable(_) => {
            if expectations.sealed.is_some() {
                return Err(AcceptanceDefect::SealedIdentity);
            }
        }
        Evaluation::Resolved(evaluation) => {
            let BaseSnapshot::Git(base) = &evaluation.base else {
                return Err(AcceptanceDefect::BaseIdentity);
            };
            if base.commit_oid != expectations.base_commit {
                return Err(AcceptanceDefect::BaseIdentity);
            }
            if let Some(expected) = &expectations.candidate_commit {
                let Snapshot::Available(CandidateSnapshot::Git(candidate)) = &evaluation.candidate
                else {
                    return Err(AcceptanceDefect::CandidateIdentity);
                };
                if candidate.commit_oid != *expected {
                    return Err(AcceptanceDefect::CandidateIdentity);
                }
            }
            if let Some(sealed) = &expectations.sealed {
                identity::accept(evaluation, &payload.controls, sealed)?;
            }
        }
    }
    let verdict =
        result_verdict(&payload.result).map_err(|_defect| AcceptanceDefect::Completeness)?;
    if u64::try_from(payload.findings.len()).map_err(|_defect| AcceptanceDefect::Shape)?
        != payload.result.finding_count
    {
        return Err(AcceptanceDefect::FindingCount);
    }
    Ok(i64::from(verdict.code()))
}
