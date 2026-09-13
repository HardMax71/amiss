use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumString};

use crate::assessment::{AssessmentEngine, AssessmentSubject, AssessmentVerdict, Nullable};
use crate::de::{Error, ErrorKind, fail};
use crate::envelope::{Envelope, Payload};
use crate::model::Digest;
use crate::semantic::producer_version_valid;

use super::evidence::PublicationEvidence;
use super::{PUBLICATION_DOCUMENT_BYTES, PublicationPlan};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/publication-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/publication-assessment-payload";

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    AsRefStr,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum PublicationReason {
    EvidenceAbsent,
    EvidenceUnbound,
    ProducerMismatch,
    DocsMismatch,
    TargetMismatch,
    SiteMismatch,
    ProductMismatch,
}

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum AssessmentEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/publication-assessment-envelope")]
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AssessmentPayloadSchema {
    #[strum(serialize = "amiss/publication-assessment-payload")]
    Current,
}

impl Payload for PublicationAssessment {
    type Schema = AssessmentEnvelopeSchema;
    const DOMAIN: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = PUBLICATION_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate_assessment(self)
    }
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
    plan: &Envelope<PublicationPlan>,
    evidence: Option<&Envelope<PublicationEvidence>>,
    engine_version: &str,
    engine_digest: Digest,
) -> Result<Vec<u8>, Error> {
    PublicationAssessment::evaluate(plan, evidence, engine_version, engine_digest)?.emit()
}

impl PublicationAssessment {
    /// Judges a validated plan and optional evidence without encoding an artifact.
    ///
    /// # Errors
    /// Refuses inconsistent input digests, invalid domain fields, or an invalid engine version.
    pub fn evaluate(
        plan: &Envelope<PublicationPlan>,
        evidence: Option<&Envelope<PublicationEvidence>>,
        engine_version: &str,
        engine_digest: Digest,
    ) -> Result<PublicationAssessment, Error> {
        if plan.payload.digest()? != plan.payload_digest {
            return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
        }
        if let Some(evidence) = evidence
            && evidence.payload.digest()? != evidence.payload_digest
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
        Ok(assessment)
    }
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
