use std::cmp::Ordering;

use strum::{AsRefStr, EnumString};

use crate::controls::decode_enum;
use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;

use super::evidence::{PublicationEvidenceEnvelope, evidence as build_evidence};
use super::{PublicationPlanEnvelope, envelope, parse_envelope, plan as build_plan};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/publication-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/publication-assessment-payload";

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum PublicationVerdict {
    Matched,
    Refuted,
    Unproven,
}

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
    pub verdict: PublicationVerdict,
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
    let (payload, payload_digest) = parse_envelope(
        bytes,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
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
            PublicationVerdict::Unproven,
            vec![PublicationReason::EvidenceAbsent],
        ),
        Some(evidence) if evidence.payload.plan_payload_digest != plan.payload_digest => (
            PublicationVerdict::Unproven,
            vec![PublicationReason::EvidenceUnbound],
        ),
        Some(evidence) if evidence.payload.producer != plan.payload.producer => (
            PublicationVerdict::Unproven,
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
                PublicationVerdict::Matched
            } else {
                PublicationVerdict::Refuted
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
    envelope(
        payload,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
    )
}

fn decode_assessment(path: &str, value: Value) -> Result<PublicationAssessment, Error> {
    let mut assessment = Obj::new(path, value)?;
    assessment.required("schema", |path, value| {
        de::const_str(path, value, ASSESSMENT_PAYLOAD_SCHEMA)
    })?;
    let (engine_version, engine_digest) = assessment.required("engine", |path, value| {
        let mut engine = Obj::new(path, value)?;
        let version = engine.required("engine_version", super::decode_producer_version)?;
        let digest = engine.required("engine_digest", de::digest)?;
        engine.finish()?;
        Ok((version, digest))
    })?;
    let (report_payload_digest, plan_payload_digest, evidence_payload_digest) = assessment
        .required("subject", |path, value| {
            let mut subject = Obj::new(path, value)?;
            let report = subject.required("report_payload_digest", de::digest)?;
            let plan = subject.required("plan_payload_digest", de::digest)?;
            let evidence_path = subject.field("evidence_payload_digest");
            let evidence = de::nullable(subject.take("evidence_payload_digest")?)
                .map(|value| de::digest(&evidence_path, value))
                .transpose()?;
            subject.finish()?;
            Ok((report, plan, evidence))
        })?;
    let verdict = assessment.required("verdict", decode_enum)?;
    let reasons_path = assessment.field("reasons");
    let reasons: Vec<PublicationReason> = de::array(&reasons_path, assessment.take("reasons")?)?
        .into_iter()
        .enumerate()
        .map(|(index, value)| decode_enum(&format!("{reasons_path}[{index}]"), value))
        .collect::<Result<Vec<_>, _>>()?;
    for pair in reasons.windows(2) {
        if let [left, right] = pair {
            match left.cmp(right) {
                Ordering::Less => {}
                Ordering::Equal => return fail(&reasons_path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(&reasons_path, ErrorKind::UnsortedSet),
            }
        }
    }
    assessment.finish()?;
    let fixed_shape = matches!(
        (verdict, evidence_payload_digest, reasons.as_slice()),
        (PublicationVerdict::Matched, Some(_), [])
            | (
                PublicationVerdict::Unproven,
                None,
                [PublicationReason::EvidenceAbsent]
            )
            | (
                PublicationVerdict::Unproven,
                Some(_),
                [PublicationReason::EvidenceUnbound | PublicationReason::ProducerMismatch],
            )
    );
    let refuted_shape = verdict == PublicationVerdict::Refuted
        && evidence_payload_digest.is_some()
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
        engine_version,
        engine_digest,
        report_payload_digest,
        plan_payload_digest,
        evidence_payload_digest,
        verdict,
        reasons,
    })
}

fn assessment_value(assessment: &PublicationAssessment) -> Value {
    object(vec![
        ("schema", text(ASSESSMENT_PAYLOAD_SCHEMA)),
        (
            "engine",
            object(vec![
                ("engine_version", text(&assessment.engine_version)),
                ("engine_digest", text(&assessment.engine_digest.to_string())),
            ]),
        ),
        (
            "subject",
            object(vec![
                (
                    "report_payload_digest",
                    text(&assessment.report_payload_digest.to_string()),
                ),
                (
                    "plan_payload_digest",
                    text(&assessment.plan_payload_digest.to_string()),
                ),
                (
                    "evidence_payload_digest",
                    assessment
                        .evidence_payload_digest
                        .map_or(Value::Null, |digest| text(&digest.to_string())),
                ),
            ]),
        ),
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
