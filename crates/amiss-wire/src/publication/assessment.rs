use garde::Validate;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentVerdict, EngineBinding, SubjectBinding, ordered};
use crate::codec::{Document, Envelope, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;

use super::evidence::PublicationEvidenceEnvelope;
use super::{PUBLICATION_DOCUMENT_BYTES, PublicationPlanEnvelope};

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

pub type PublicationAssessmentEnvelope = Envelope<PublicationAssessment>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationAssessment {
    pub schema: Schema<Self>,
    #[garde(dive)]
    pub engine: EngineBinding,
    pub subject: SubjectBinding,
    pub verdict: AssessmentVerdict,
    pub reasons: Vec<PublicationReason>,
}

/// Parses one closed, digest-bound offline publication assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// engine identity, unsorted reasons, an inconsistent verdict, or a payload
/// digest mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<PublicationAssessmentEnvelope, Error> {
    PublicationAssessmentEnvelope::parse(bytes)
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
) -> Result<PublicationAssessmentEnvelope, Error> {
    plan.verify("$.plan")?;
    if let Some(evidence) = evidence {
        evidence.verify("$.evidence")?;
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
    PublicationAssessmentEnvelope::seal(PublicationAssessment {
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
        reasons,
    })
}

impl Document for PublicationAssessment {
    const PAYLOAD_SCHEMA: &'static str = ASSESSMENT_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = ASSESSMENT_ENVELOPE_SCHEMA;
    const LIMIT: u64 = PUBLICATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        ordered(&format!("{root}.reasons"), &self.reasons, 7, |reason| {
            reason
        })?;
        let Self {
            verdict,
            reasons,
            subject,
            ..
        } = self;
        let verdict = *verdict;
        let fixed_shape = matches!(
            (verdict, subject.evidence_payload_digest, reasons.as_slice()),
            (AssessmentVerdict::Matched, Some(_), [])
                | (
                    AssessmentVerdict::Unproven,
                    None,
                    [PublicationReason::EvidenceAbsent]
                )
                | (
                    AssessmentVerdict::Unproven,
                    Some(_),
                    [PublicationReason::EvidenceUnbound | PublicationReason::ProducerMismatch],
                )
        );
        let refuted_shape = verdict == AssessmentVerdict::Refuted
            && subject.evidence_payload_digest.is_some()
            && !reasons.is_empty()
            && reasons.iter().all(|reason| {
                matches!(
                    reason,
                    PublicationReason::DocsMismatch
                        | PublicationReason::TargetMismatch
                        | PublicationReason::SiteMismatch
                        | PublicationReason::ProductMismatch
                )
            });
        if fixed_shape || refuted_shape {
            Ok(())
        } else {
            fail(root, ErrorKind::Inconsistent)
        }
    }
}
