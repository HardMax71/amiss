use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumString};

use crate::assessment::{AssessmentEngine, AssessmentSubject, Nullable};
use crate::de::{Error, ErrorKind, fail};
use crate::envelope::{Envelope, Payload};
use crate::model::Digest;
use crate::semantic::producer_version_valid;

use super::evidence::{RelationEvidence, RelationProjectionSlot};
use super::{RELATION_DOCUMENT_BYTES, RelationPlan};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/relation-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/relation-assessment-payload";

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    AsRefStr,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RelationVerdict {
    Aligned,
    IntroducedDrift,
    PreExistingDrift,
    ResolvedDrift,
    Unproven,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    AsRefStr,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RelationReason {
    EvidenceAbsent,
    EvidenceUnbound,
    RoleMismatch,
    ProjectionUnproven,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationAssessment {
    pub schema: AssessmentPayloadSchema,
    pub engine: AssessmentEngine,
    pub subject: AssessmentSubject,
    pub verdict: RelationVerdict,
    pub reason: Nullable<RelationReason>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum AssessmentEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/relation-assessment-envelope")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AssessmentPayloadSchema {
    #[strum(serialize = "amiss/relation-assessment-payload")]
    Current,
}

impl Payload for RelationAssessment {
    type Schema = AssessmentEnvelopeSchema;
    const DOMAIN: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = RELATION_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate_assessment(self)
    }
}

/// Judges the equality transition of two relation subjects.
///
/// All four projection slots must be complete before the assessment can
/// distinguish aligned, introduced, pre-existing, and resolved drift. The
/// result compares roles symmetrically and never assigns either one authority.
///
/// # Errors
///
/// Fails when either typed envelope no longer reproduces its own digest, a
/// public field violates its source contract, or the engine version is not a
/// bounded producer version.
pub fn assess(
    plan: &Envelope<RelationPlan>,
    evidence: Option<&Envelope<RelationEvidence>>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Vec<u8>, Error> {
    RelationAssessment::evaluate(plan, evidence, engine_version, engine_digest)?.emit()
}

impl RelationAssessment {
    /// Judges a validated plan and optional evidence without encoding an artifact.
    ///
    /// # Errors
    /// Refuses inconsistent input digests, invalid domain fields, or an invalid engine version.
    pub fn evaluate(
        plan: &Envelope<RelationPlan>,
        evidence: Option<&Envelope<RelationEvidence>>,
        engine_version: &str,
        engine_digest: Digest,
    ) -> Result<RelationAssessment, Error> {
        if plan.payload.digest()? != plan.payload_digest {
            return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
        }
        if let Some(evidence) = evidence
            && evidence.payload.digest()? != evidence.payload_digest
        {
            return fail("$.evidence.payload_digest", ErrorKind::DigestMismatch);
        }

        let judgment = evidence
            .ok_or(RelationReason::EvidenceAbsent)
            .and_then(|evidence| {
                if evidence.payload.plan_payload_digest != plan.payload_digest {
                    return Err(RelationReason::EvidenceUnbound);
                }
                if evidence
                    .payload
                    .subjects
                    .iter()
                    .zip(&plan.payload.subjects)
                    .any(|(observed, planned)| observed.role != planned.role)
                {
                    return Err(RelationReason::RoleMismatch);
                }
                let [left, right] = &evidence.payload.subjects;
                let [
                    RelationProjectionSlot::Projected(left_base),
                    RelationProjectionSlot::Projected(right_base),
                    RelationProjectionSlot::Projected(left_candidate),
                    RelationProjectionSlot::Projected(right_candidate),
                ] = [left.base, right.base, left.candidate, right.candidate]
                else {
                    return Err(RelationReason::ProjectionUnproven);
                };
                Ok(
                    match (left_base == right_base, left_candidate == right_candidate) {
                        (true, true) => RelationVerdict::Aligned,
                        (true, false) => RelationVerdict::IntroducedDrift,
                        (false, false) => RelationVerdict::PreExistingDrift,
                        (false, true) => RelationVerdict::ResolvedDrift,
                    },
                )
            });
        let (verdict, reason) = judgment.map_or_else(
            |reason| (RelationVerdict::Unproven, Some(reason)),
            |verdict| (verdict, None),
        );
        let assessment = RelationAssessment {
            schema: AssessmentPayloadSchema::Current,
            engine: AssessmentEngine {
                engine_version: engine_version.to_owned(),
                engine_digest,
            },
            subject: AssessmentSubject {
                report_payload_digest: plan.payload.report_payload_digest,
                plan_payload_digest: plan.payload_digest,
                evidence_payload_digest: evidence.map_or(Nullable::Null, |evidence| {
                    Nullable::Value(evidence.payload_digest)
                }),
            },
            verdict,
            reason: reason.map_or(Nullable::Null, Nullable::Value),
        };
        validate_assessment(&assessment)?;
        Ok(assessment)
    }
}

fn validate_assessment(assessment: &RelationAssessment) -> Result<(), Error> {
    if !producer_version_valid(&assessment.engine.engine_version) {
        return fail("$.payload.engine.engine_version", ErrorKind::InvalidValue);
    }
    let valid = match assessment.reason {
        Nullable::Null => {
            assessment.verdict != RelationVerdict::Unproven
                && matches!(
                    assessment.subject.evidence_payload_digest,
                    Nullable::Value(_)
                )
        }
        Nullable::Value(RelationReason::EvidenceAbsent) => {
            assessment.verdict == RelationVerdict::Unproven
                && assessment.subject.evidence_payload_digest == Nullable::Null
        }
        Nullable::Value(
            RelationReason::EvidenceUnbound
            | RelationReason::RoleMismatch
            | RelationReason::ProjectionUnproven,
        ) => {
            assessment.verdict == RelationVerdict::Unproven
                && matches!(
                    assessment.subject.evidence_payload_digest,
                    Nullable::Value(_)
                )
        }
    };
    if !valid {
        return fail("$.payload", ErrorKind::Inconsistent);
    }
    Ok(())
}
