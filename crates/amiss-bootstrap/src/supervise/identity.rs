use amiss_wire::model::{BranchRef, UtcInstant};
use amiss_wire::report::model::{Controls, IdentityPreimage, ResolvedEvaluation};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema};
use sha2::Digest as _;

use super::{AcceptanceDefect, SealedExpectations, controls};

pub(super) fn accept(
    evaluation: &ResolvedEvaluation,
    controls: &Controls,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    if evaluation.candidate_ref.as_ref().map(BranchRef::as_str)
        != Some(expected.candidate_ref.as_str())
        || evaluation.target_ref.as_ref().map(BranchRef::as_str)
            != Some(expected.target_ref.as_str())
        || !evaluation.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let preimage = IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let identity_digest = {
        let mut writer = digest_io::IoWrapper(
            sha2::Sha256::new_with_prefix(CANDIDATE_IDENTITY_DOMAIN).chain_update([0_u8]),
        );
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
            .map(|()| amiss_wire::model::Digest::from(writer.0.finalize().0))
    }
    .map_err(|_defect| AcceptanceDefect::SealedIdentity)?;
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
