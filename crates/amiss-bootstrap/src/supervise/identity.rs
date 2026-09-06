use amiss_wire::digest::hj_serde;
use amiss_wire::report::model::{IdentityPayload, IdentityPreimage, ReportEnvelope};
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

pub(super) fn accept(wire: &[u8], expected: &SealedExpectations) -> Result<(), AcceptanceDefect> {
    let mut deserializer = serde_json::Deserializer::from_slice(wire);
    // The caller already applied the strict parser's nesting limit.
    deserializer.disable_recursion_limit();
    let envelope =
        ReportEnvelope::<IdentityPayload<serde_json::Value>>::deserialize(&mut deserializer)
            .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    let evaluation = envelope.payload.evaluation;
    let references = SealedReferences::deserialize(&evaluation)
        .map_err(|_defect| AcceptanceDefect::SealedIdentity)?;
    if references.candidate_ref != expected.candidate_ref
        || references.target_ref != expected.target_ref
        || !references.trusted_time
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let instant = EvaluationInstant::deserialize(&evaluation);
    let preimage = IdentityPreimage {
        evaluation: serde_json::from_value(evaluation)
            .map_err(|_defect| AcceptanceDefect::SealedIdentity)?,
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
        wire,
        instant.evaluation_instant.as_deref(),
        identity_digest,
        expected,
    )
}
