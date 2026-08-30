use strum::{AsRefStr, EnumString};

use crate::assessment::{AssessmentVerdict, bindings_value, decode_bindings};
use crate::controls::decode_enum;
use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;

use super::evidence::{PublicationEvidenceEnvelope, evidence as build_evidence};
use super::{PUBLICATION_DOCUMENT_BYTES, PublicationPlanEnvelope, plan as build_plan};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/publication-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/publication-assessment-payload";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationAssessmentEnvelope {
    pub payload: PublicationAssessment,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationAssessment {
    pub engine_version: String,
    pub engine_digest: Digest,
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Option<Digest>,
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
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        PUBLICATION_DOCUMENT_BYTES,
        decode_assessment,
    )?;
    Ok(PublicationAssessmentEnvelope {
        payload,
        payload_digest,
    })
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
) -> Result<Value, Error> {
    let rebuilt_plan = build_plan(&plan.payload)?;
    if rebuilt_plan.text("payload_digest") != Some(&plan.payload_digest.to_string()) {
        return fail("$.plan.payload_digest", ErrorKind::DigestMismatch);
    }
    if let Some(evidence) = evidence {
        let rebuilt_evidence = build_evidence(&evidence.payload)?;
        if rebuilt_evidence.text("payload_digest") != Some(&evidence.payload_digest.to_string()) {
            return fail("$.evidence.payload_digest", ErrorKind::DigestMismatch);
        }
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
        engine_version: engine_version.to_owned(),
        engine_digest,
        report_payload_digest: plan.payload.report_payload_digest,
        plan_payload_digest: plan.payload_digest,
        evidence_payload_digest: evidence.map(|evidence| evidence.payload_digest),
        verdict,
        reasons,
    };
    let payload = assessment_value(&assessment);
    let _validated = decode_assessment("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        PUBLICATION_DOCUMENT_BYTES,
    )
}

fn decode_assessment(path: &str, value: Value) -> Result<PublicationAssessment, Error> {
    let mut assessment = Obj::new(path, value)?;
    assessment.required("schema", |path, value| {
        de::const_str(path, value, ASSESSMENT_PAYLOAD_SCHEMA)
    })?;
    let bindings = decode_bindings(&mut assessment)?;
    let verdict = assessment.required("verdict", decode_enum)?;
    let reasons_path = assessment.field("reasons");
    let reasons = de::sorted_items(
        &reasons_path,
        assessment.take("reasons")?,
        7,
        decode_enum,
        |reason| reason,
    )?;
    assessment.finish()?;
    let fixed_shape = matches!(
        (
            verdict,
            bindings.evidence_payload_digest,
            reasons.as_slice()
        ),
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
        && bindings.evidence_payload_digest.is_some()
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
    let valid = fixed_shape || refuted_shape;
    if !valid {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(PublicationAssessment {
        engine_version: bindings.engine_version,
        engine_digest: bindings.engine_digest,
        report_payload_digest: bindings.report_payload_digest,
        plan_payload_digest: bindings.plan_payload_digest,
        evidence_payload_digest: bindings.evidence_payload_digest,
        verdict,
        reasons,
    })
}

fn assessment_value(assessment: &PublicationAssessment) -> Value {
    let (engine, subject) = bindings_value(
        &assessment.engine_version,
        assessment.engine_digest,
        assessment.report_payload_digest,
        assessment.plan_payload_digest,
        assessment.evidence_payload_digest,
    );
    object(vec![
        ("schema", text(ASSESSMENT_PAYLOAD_SCHEMA)),
        ("engine", engine),
        ("subject", subject),
        ("verdict", text(assessment.verdict.as_ref())),
        (
            "reasons",
            Value::array(
                assessment
                    .reasons
                    .iter()
                    .map(|reason| text(reason.as_ref()))
                    .collect(),
            ),
        ),
    ])
}
