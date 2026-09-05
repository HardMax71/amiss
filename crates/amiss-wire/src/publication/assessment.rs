use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentEngine, AssessmentSubject, AssessmentVerdict, Nullable};
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json;
use crate::semantic::producer_version_valid;

use super::evidence::{PublicationEvidenceEnvelope, evidence_payload_digest};
use super::{PUBLICATION_DOCUMENT_BYTES, PublicationPlanEnvelope, plan_payload_digest};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/publication-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/publication-assessment-payload";

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString, Serialize, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum PublicationReason {
    EvidenceAbsent,
    EvidenceUnbound,
    ProducerMismatch,
    DocsMismatch,
    TargetMismatch,
    SiteMismatch,
    ProductMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAssessmentEnvelope {
    pub schema: AssessmentEnvelopeSchema,
    pub payload: PublicationAssessment,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssessmentEnvelopeSchema {
    #[serde(rename = "amiss/publication-assessment-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAssessment {
    pub schema: AssessmentPayloadSchema,
    pub engine: AssessmentEngine,
    pub subject: AssessmentSubject,
    pub verdict: AssessmentVerdict,
    pub reasons: Vec<PublicationReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssessmentPayloadSchema {
    #[serde(rename = "amiss/publication-assessment-payload")]
    Current,
}

/// Parses one closed, digest-bound offline publication assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// engine identity, unsorted reasons, an inconsistent verdict, or a payload
/// digest mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<PublicationAssessmentEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: PublicationAssessmentEnvelope = de::deserialize_json(bytes)?;
    if assessment_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Judges one publication plan against optional provider-normalized evidence.
///
/// A missing receipt, a receipt for another plan, or one from another selected
/// producer stays unproven. Only bound evidence from the selected producer can
/// match or refute the planned docs, target, site, and product facts.
///
/// # Errors
///
/// Fails when either typed envelope no longer reproduces its own digest, when a
/// public field violates its source contract, or when the engine version is not
/// a bounded producer version.
pub fn assess(
    plan: &PublicationPlanEnvelope,
    evidence: Option<&PublicationEvidenceEnvelope>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Vec<u8>, Error> {
    if plan_payload_digest(&plan.payload)? != plan.payload_digest {
        return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
    }
    if let Some(evidence) = evidence
        && evidence_payload_digest(&evidence.payload)? != evidence.payload_digest
    {
        return fail("$.evidence.payload_digest", ErrorKind::DigestMismatch);
    }

    let (verdict, reasons) = match evidence {
        None => (
            AssessmentVerdict::Unproven,
            vec![PublicationReason::EvidenceAbsent],
        ),
        Some(evidence) if evidence.payload.plan_payload_digest != plan.payload_digest => (
            AssessmentVerdict::Unproven,
            vec![PublicationReason::EvidenceUnbound],
        ),
        Some(evidence) if evidence.payload.producer != plan.payload.producer => (
            AssessmentVerdict::Unproven,
            vec![PublicationReason::ProducerMismatch],
        ),
        Some(evidence) => {
            let reasons: Vec<_> = [
                (
                    evidence.payload.docs != plan.payload.docs,
                    PublicationReason::DocsMismatch,
                ),
                (
                    evidence.payload.target != plan.payload.target,
                    PublicationReason::TargetMismatch,
                ),
                (
                    evidence.payload.site != plan.payload.site,
                    PublicationReason::SiteMismatch,
                ),
                (
                    evidence.payload.product != plan.payload.product,
                    PublicationReason::ProductMismatch,
                ),
            ]
            .into_iter()
            .filter_map(|(different, reason)| different.then_some(reason))
            .collect();
            let verdict = if reasons.is_empty() {
                AssessmentVerdict::Matched
            } else {
                AssessmentVerdict::Refuted
            };
            (verdict, reasons)
        }
    };
    let assessment = PublicationAssessment {
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
        reasons,
    };
    let payload_digest = assessment_payload_digest(&assessment)?;
    let document = PublicationAssessmentEnvelope {
        schema: AssessmentEnvelopeSchema::Current,
        payload: assessment,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    Ok(canonical)
}

fn assessment_payload_digest(assessment: &PublicationAssessment) -> Result<Digest, Error> {
    validate_assessment(assessment)?;
    serde_json_canonicalizer::to_vec(assessment)
        .map(|canonical| hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn validate_assessment(assessment: &PublicationAssessment) -> Result<(), Error> {
    producer_version_valid(&assessment.engine.engine_version)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload.engine.engine_version", ErrorKind::InvalidValue))?;
    (assessment.reasons.len() <= 7)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload.reasons", ErrorKind::LimitExceeded))?;
    assessment
        .reasons
        .iter()
        .zip(assessment.reasons.iter().skip(1))
        .try_for_each(|(previous, current)| match previous.cmp(current) {
            Ordering::Less => Ok(()),
            Ordering::Equal => fail("$.payload.reasons", ErrorKind::DuplicateMember),
            Ordering::Greater => fail("$.payload.reasons", ErrorKind::UnsortedSet),
        })?;
    let fixed_shape = matches!(
        (
            assessment.verdict,
            assessment.subject.evidence_payload_digest,
            assessment.reasons.as_slice()
        ),
        (AssessmentVerdict::Matched, Nullable::Value(_), [])
            | (
                AssessmentVerdict::Unproven,
                Nullable::Null,
                [PublicationReason::EvidenceAbsent]
            )
            | (
                AssessmentVerdict::Unproven,
                Nullable::Value(_),
                [PublicationReason::EvidenceUnbound | PublicationReason::ProducerMismatch],
            )
    );
    let refuted_shape = assessment.verdict == AssessmentVerdict::Refuted
        && matches!(
            assessment.subject.evidence_payload_digest,
            Nullable::Value(_)
        )
        && !assessment.reasons.is_empty()
        && assessment.reasons.iter().all(|reason| {
            matches!(
                reason,
                PublicationReason::DocsMismatch
                    | PublicationReason::TargetMismatch
                    | PublicationReason::SiteMismatch
                    | PublicationReason::ProductMismatch
            )
        });
    (fixed_shape || refuted_shape)
        .then_some(())
        .ok_or_else(|| Error::new("$.payload", ErrorKind::Inconsistent))
}
