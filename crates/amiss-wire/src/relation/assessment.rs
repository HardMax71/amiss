use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentEngine, AssessmentSubject, Nullable};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json::{self, Value};
use crate::semantic::producer_version_valid;

use super::evidence::{RelationEvidenceEnvelope, RelationProjectionSlot, evidence_payload_digest};
use super::{RELATION_DOCUMENT_BYTES, RelationPlanEnvelope, plan_payload_digest};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/relation-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/relation-assessment-payload";

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RelationVerdict {
    Aligned,
    IntroducedDrift,
    PreExistingDrift,
    ResolvedDrift,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RelationReason {
    EvidenceAbsent,
    EvidenceUnbound,
    RoleMismatch,
    ProjectionUnproven,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationAssessmentEnvelope {
    pub payload: RelationAssessment,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationAssessment {
    pub engine: AssessmentEngine,
    pub subject: AssessmentSubject,
    pub verdict: RelationVerdict,
    pub reason: Nullable<RelationReason>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum AssessmentEnvelope<T> {
    #[serde(rename = "amiss/relation-assessment-envelope")]
    Current { payload: T, payload_digest: Digest },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum AssessmentPayload<T> {
    #[serde(rename = "amiss/relation-assessment-payload")]
    Current(T),
}

/// Parses one closed, digest-bound relation transition assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// engine identity, an inconsistent verdict/reason pair, or a payload digest
/// mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<RelationAssessmentEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: AssessmentEnvelope<AssessmentPayload<RelationAssessment>> =
        serde_json::from_slice(bytes)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let AssessmentEnvelope::Current {
        payload,
        payload_digest,
    } = document;
    let AssessmentPayload::Current(payload) = payload;
    validate_assessment(&payload)?;
    let canonical = serde_json_canonicalizer::to_vec(&AssessmentPayload::Current(&payload))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
    if hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical) != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(RelationAssessmentEnvelope {
        payload,
        payload_digest,
    })
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
    plan: &RelationPlanEnvelope,
    evidence: Option<&RelationEvidenceEnvelope>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Value, Error> {
    if plan_payload_digest(&plan.payload)? != plan.payload_digest {
        return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
    }
    if let Some(evidence) = evidence
        && evidence_payload_digest(&evidence.payload)? != evidence.payload_digest
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
    let payload = AssessmentPayload::Current(&assessment);
    let canonical_payload = serde_json_canonicalizer::to_vec(&payload)
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
    let document = AssessmentEnvelope::Current {
        payload,
        payload_digest: hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical_payload),
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
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
