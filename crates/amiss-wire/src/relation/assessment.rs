use strum::{AsRefStr, EnumString};

use crate::assessment::{bindings_value, decode_bindings};
use crate::controls::decode_enum;
use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;

use super::evidence::{
    RelationEvidenceEnvelope, RelationProjectionSlot, evidence as build_evidence,
};
use super::{RELATION_DOCUMENT_BYTES, RelationPlanEnvelope, plan as build_plan};

pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/relation-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/relation-assessment-payload";

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum RelationVerdict {
    Aligned,
    IntroducedDrift,
    PreExistingDrift,
    ResolvedDrift,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationAssessment {
    pub engine_version: String,
    pub engine_digest: Digest,
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Option<Digest>,
    pub verdict: RelationVerdict,
    pub reason: Option<RelationReason>,
}

/// Parses one closed, digest-bound relation transition assessment.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// engine identity, an inconsistent verdict/reason pair, or a payload digest
/// mismatch.
pub fn parse_assessment(bytes: &[u8]) -> Result<RelationAssessmentEnvelope, Error> {
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
        decode_assessment,
    )?;
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
        engine_version: engine_version.to_owned(),
        engine_digest,
        report_payload_digest: plan.payload.report_payload_digest,
        plan_payload_digest: plan.payload_digest,
        evidence_payload_digest: evidence.map(|evidence| evidence.payload_digest),
        verdict,
        reason,
    };
    let payload = assessment_value(&assessment);
    let _validated = decode_assessment("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        ASSESSMENT_ENVELOPE_SCHEMA,
        ASSESSMENT_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
    )
}

fn decode_assessment(path: &str, value: Value) -> Result<RelationAssessment, Error> {
    let mut assessment = Obj::new(path, value)?;
    assessment.required("schema", |path, value| {
        de::const_str(path, value, ASSESSMENT_PAYLOAD_SCHEMA)
    })?;
    let bindings = decode_bindings(&mut assessment)?;
    let verdict = assessment.required("verdict", decode_enum)?;
    let reason = assessment.required("reason", |path, value| {
        de::decode_nullable(path, value, decode_enum)
    })?;
    assessment.finish()?;

    let valid = matches!(
        (verdict, reason, bindings.evidence_payload_digest),
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
    if !valid {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(RelationAssessment {
        engine_version: bindings.engine_version,
        engine_digest: bindings.engine_digest,
        report_payload_digest: bindings.report_payload_digest,
        plan_payload_digest: bindings.plan_payload_digest,
        evidence_payload_digest: bindings.evidence_payload_digest,
        verdict,
        reason,
    })
}

fn assessment_value(assessment: &RelationAssessment) -> Value {
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
            "reason",
            assessment
                .reason
                .map_or(Value::Null, |reason| text(reason.as_ref())),
        ),
    ])
}
