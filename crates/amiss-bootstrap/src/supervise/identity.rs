use amiss_wire::digest::hj_serde;
use amiss_wire::model::{BranchRef, UtcInstant};
use amiss_wire::report::model::{IdentityPayload, IdentityPreimage, ResolvedEvaluation};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema};
use serde::Deserialize;

use super::{AcceptanceDefect, SealedExpectations, controls};

pub(super) fn accept(
    payload: &serde_json::Value,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let evaluation = IdentityPayload::<ResolvedEvaluation>::deserialize(payload)
        .map_err(|_defect| AcceptanceDefect::SealedIdentity)?
        .evaluation;
    if evaluation.candidate_ref.as_ref().map(BranchRef::as_str)
        != Some(expected.candidate_ref.as_str())
        || evaluation.target_ref.as_ref().map(BranchRef::as_str)
            != Some(expected.target_ref.as_str())
        || !evaluation.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let preimage = IdentityPreimage {
        evaluation: &evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let identity_digest = hj_serde(CANDIDATE_IDENTITY_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
    })
    .map_err(|_defect| AcceptanceDefect::SealedIdentity)?;
    if identity_digest != expected.candidate_identity_digest {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    controls::accept(
        payload,
        evaluation
            .evaluation_instant
            .as_ref()
            .map(UtcInstant::as_str),
        identity_digest,
        expected,
    )
}
