use amiss_wire::envelope::Payload as _;
use amiss_wire::report::model::{BaseSnapshot, Evaluation, ReportPayload, Snapshot};
use amiss_wire::report::{ReportDefect, result_verdict};
use amiss_wire::requests::CandidateSnapshot;

use super::{AcceptanceDefect, Expectations, identity};

/// Checks a full report's shape, canonical bytes and bindings, returning its exit class.
///
/// # Errors
///
/// Requires LF framing and a closed typed envelope in its own spelling, with its
/// payload digest and result tuple, then checks canonical bytes, engine/base/candidate
/// identities, sealed controls and finding count in that order.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<i64, AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    let envelope = <ReportPayload>::parse(trimmed).map_err(|defect| match defect {
        ReportDefect::Noncanonical => AcceptanceDefect::Noncanonical,
        ReportDefect::DigestMismatch => AcceptanceDefect::PayloadDigest,
        ReportDefect::InvalidResult => AcceptanceDefect::Completeness,
        ReportDefect::NotAReport | ReportDefect::Incomplete | ReportDefect::MalformedExternal => {
            AcceptanceDefect::Shape
        }
    })?;
    let verdict = result_verdict(&envelope.payload.result)
        .map_err(|_defect| AcceptanceDefect::Completeness)?;
    if serde_json_canonicalizer::to_vec(&envelope).map_err(|_defect| AcceptanceDefect::Shape)?
        != trimmed
    {
        return Err(AcceptanceDefect::Noncanonical);
    }
    let payload = envelope.payload;
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
    if u64::try_from(payload.findings.len()).map_err(|_defect| AcceptanceDefect::Shape)?
        != payload.result.finding_count
    {
        return Err(AcceptanceDefect::FindingCount);
    }
    Ok(i64::from(verdict.code()))
}
