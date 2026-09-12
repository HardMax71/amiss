use garde::Validate;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, EnumString};

use crate::assessment::{EngineBinding, SubjectBinding};
use crate::codec::{Document, Envelope, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;

use super::evidence::RelationEvidenceEnvelope;
use super::{RELATION_DOCUMENT_BYTES, RelationPlanEnvelope};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/relation-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/relation-assessment-payload";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumIter, EnumString, Serialize, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RelationVerdict {
    Aligned,
    IntroducedDrift,
    PreExistingDrift,
    ResolvedDrift,
    Unproven,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumIter, EnumString, Serialize, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RelationReason {
    EvidenceAbsent,
    EvidenceUnbound,
    RoleMismatch,
    ProjectionUnproven,
}

pub type RelationAssessmentEnvelope = Envelope<RelationAssessment>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationAssessment {
    pub schema: Schema<Self>,
    #[garde(dive)]
    pub engine: EngineBinding,
    pub subject: SubjectBinding,
    pub verdict: RelationVerdict,
    #[serde(deserialize_with = "crate::codec::nullable")]
    pub reason: Option<RelationReason>,
}

impl Document for RelationAssessment {
    const PAYLOAD_SCHEMA: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = ASSESSMENT_ENVELOPE_SCHEMA;
    const LIMIT: u64 = RELATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        let valid = matches!(
            (
                self.verdict,
                self.reason,
                self.subject.evidence_payload_digest
            ),
            (
                RelationVerdict::Aligned
                    | RelationVerdict::IntroducedDrift
                    | RelationVerdict::PreExistingDrift
                    | RelationVerdict::ResolvedDrift,
                None,
                Some(_),
            ) | (
                RelationVerdict::Unproven,
                Some(RelationReason::EvidenceAbsent),
                None
            ) | (
                RelationVerdict::Unproven,
                Some(
                    RelationReason::EvidenceUnbound
                        | RelationReason::RoleMismatch
                        | RelationReason::ProjectionUnproven,
                ),
                Some(_),
            )
        );
        if valid {
            Ok(())
        } else {
            fail(root, ErrorKind::Inconsistent)
        }
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
/// Fails when either typed envelope no longer reproduces its own digest, or
/// the engine version is not a bounded producer version.
pub fn assess(
    plan: &RelationPlanEnvelope,
    evidence: Option<&RelationEvidenceEnvelope>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<RelationAssessmentEnvelope, Error> {
    plan.verify("$.plan")?;
    if let Some(evidence) = evidence {
        evidence.verify("$.evidence")?;
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
            let ((left_base, right_base), (left_candidate, right_candidate)) = left
                .base
                .zip(right.base)
                .zip(left.candidate.zip(right.candidate))
                .ok_or(RelationReason::ProjectionUnproven)?;
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
    RelationAssessmentEnvelope::seal(RelationAssessment {
        schema: Schema::default(),
        engine: EngineBinding {
            engine_version: engine_version.to_owned(),
            engine_digest,
        },
        subject: SubjectBinding {
            report_payload_digest: plan.payload.report_payload_digest,
            plan_payload_digest: plan.payload_digest,
            evidence_payload_digest: evidence.map(|evidence| evidence.payload_digest),
        },
        verdict,
        reason,
    })
}

/// Reads a bounded relation assessment and verifies its judgment shape and digest.
///
/// # Errors
///
/// The assessment violates its contract or payload digest binding.
pub fn parse_assessment(bytes: &[u8]) -> Result<RelationAssessmentEnvelope, Error> {
    RelationAssessmentEnvelope::parse(bytes)
}
