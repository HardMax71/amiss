use amiss_wire::digest::hj_serde;
use amiss_wire::report::model::{IdentityPayload, IdentityPreimage};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema};
use serde::{Deserialize, de::IgnoredAny};

use super::{AcceptanceDefect, SealedExpectations, controls};

#[derive(Deserialize)]
struct SealedReferences {
    candidate_ref: String,
    target_ref: String,
    trusted_time: bool,
    #[serde(flatten)]
    _extensions: IgnoredAny,
}

#[derive(Deserialize)]
struct EvaluationInstant {
    evaluation_instant: Option<String>,
}

pub(super) fn accept(
    payload: &serde_json::Value,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    let references = IdentityPayload::<SealedReferences>::deserialize(payload)
        .map_err(|_defect| AcceptanceDefect::SealedIdentity)?
        .evaluation;
    if references.candidate_ref != expected.candidate_ref
        || references.target_ref != expected.target_ref
        || !references.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let instant = IdentityPayload::<EvaluationInstant>::deserialize(payload);
    let preimage = IdentityPreimage {
        evaluation: IdentityPayload::deserialize(payload)
            .map_err(|_defect| AcceptanceDefect::SealedIdentity)?
            .evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let identity_digest = hj_serde(CANDIDATE_IDENTITY_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
    })
    .map_err(|_defect| AcceptanceDefect::SealedIdentity)?;
    if identity_digest != expected.candidate_identity_digest {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let instant = instant.map_err(|_defect| AcceptanceDefect::SealedControls)?;
    controls::accept(
        payload,
        instant.evaluation.evaluation_instant.as_deref(),
        identity_digest,
        expected,
    )
}
