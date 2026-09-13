use amiss_wire::envelope::document_digest;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::model::{Controls, IdentityPreimage, ResolvedEvaluation};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema};

use super::{AcceptanceDefect, SealedExpectations, controls};

pub(super) fn accept(
    evaluation: &ResolvedEvaluation,
    controls: &Controls,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    if evaluation.candidate_ref.as_ref() != Some(&expected.candidate_ref)
        || evaluation.target_ref.as_ref() != Some(&expected.target_ref)
        || !evaluation.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let preimage = IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let identity_digest = document_digest(CANDIDATE_IDENTITY_DOMAIN, &preimage)
        .ok_or(AcceptanceDefect::SealedIdentity)?;
    if identity_digest != expected.candidate_identity_digest {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    controls::accept(
        controls,
        evaluation
            .evaluation_instant
            .as_ref()
            .map(UtcInstant::as_str),
        identity_digest,
        expected,
    )
}
