use crate::controls::value::{nonnegative_safe_integer, object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;
use crate::publication::decode_identity;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationEvidenceEnvelope {
    pub payload: RelationEvidence,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationEvidence {
    pub plan_payload_digest: Digest,
    pub subjects: [RelationEvidenceSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationEvidenceSubject {
    pub role: ArtifactId,
    pub base: Option<RelationProjectedValue>,
    pub candidate: Option<RelationProjectedValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationProjectedValue {
    pub value_digest: Digest,
    pub value_bytes: u64,
}

/// Parses one closed, digest-bound set of four relation projections.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or digest, reordered or repeated subject roles, an unsafe byte
/// count, or a payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<RelationEvidenceEnvelope, Error> {
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
        decode_evidence,
    )?;
    Ok(RelationEvidenceEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for four relation projection slots.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar
/// [`parse_evidence`] enforces or the encoded document exceeds its byte
/// ceiling.
pub fn evidence(input: &RelationEvidence) -> Result<Value, Error> {
    let payload = evidence_value(input)?;
    let _validated = decode_evidence("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
    )
}

fn decode_evidence(path: &str, value: Value) -> Result<RelationEvidence, Error> {
    let mut evidence = Obj::new(path, value)?;
    evidence.required("schema", |path, value| {
        de::const_str(path, value, EVIDENCE_PAYLOAD_SCHEMA)
    })?;
    let plan_payload_digest = evidence.required("plan_payload_digest", de::digest)?;
    let subjects_path = evidence.field("subjects");
    let subjects: [RelationEvidenceSubject; 2] =
        de::array(&subjects_path, evidence.take("subjects")?)?
            .into_iter()
            .enumerate()
            .map(|(index, value)| decode_subject(&format!("{subjects_path}[{index}]"), value))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_subjects: Vec<RelationEvidenceSubject>| {
                Error::new(&subjects_path, ErrorKind::InvalidValue)
            })?;
    evidence.finish()?;

    let [left, right] = &subjects;
    if left.role >= right.role {
        return fail(
            &subjects_path,
            if left.role == right.role {
                ErrorKind::DuplicateMember
            } else {
                ErrorKind::UnsortedSet
            },
        );
    }
    Ok(RelationEvidence {
        plan_payload_digest,
        subjects,
    })
}

fn decode_subject(path: &str, value: Value) -> Result<RelationEvidenceSubject, Error> {
    de::closed_object(path, value, |subject| {
        let decode_projection = |path: &str, value: Value| {
            de::decode_nullable(path, value, |path, value| {
                de::closed_object(path, value, |projected| {
                    let value_digest = projected.required("value_digest", de::digest)?;
                    let bytes_path = projected.field("value_bytes");
                    let value_bytes =
                        u64::try_from(de::integer(&bytes_path, projected.take("value_bytes")?)?)
                            .map_err(|_negative| {
                                Error::new(&bytes_path, ErrorKind::InvalidValue)
                            })?;
                    Ok(RelationProjectedValue {
                        value_digest,
                        value_bytes,
                    })
                })
            })
        };
        Ok(RelationEvidenceSubject {
            role: subject.required("role", decode_identity)?,
            base: subject.required("base", decode_projection)?,
            candidate: subject.required("candidate", decode_projection)?,
        })
    })
}

fn evidence_value(evidence: &RelationEvidence) -> Result<Value, Error> {
    let subjects = evidence
        .subjects
        .iter()
        .enumerate()
        .map(|(index, subject)| subject_value(index, subject))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(object(vec![
        ("schema", text(EVIDENCE_PAYLOAD_SCHEMA)),
        (
            "plan_payload_digest",
            text(&evidence.plan_payload_digest.to_string()),
        ),
        ("subjects", Value::array(subjects)),
    ]))
}

fn subject_value(index: usize, subject: &RelationEvidenceSubject) -> Result<Value, Error> {
    Ok(object(vec![
        ("role", text(subject.role.as_str())),
        (
            "base",
            projected_value(&format!("$.payload.subjects[{index}].base"), subject.base)?,
        ),
        (
            "candidate",
            projected_value(
                &format!("$.payload.subjects[{index}].candidate"),
                subject.candidate,
            )?,
        ),
    ]))
}

fn projected_value(path: &str, projected: Option<RelationProjectedValue>) -> Result<Value, Error> {
    let Some(projected) = projected else {
        return Ok(Value::Null);
    };
    Ok(object(vec![
        ("value_digest", text(&projected.value_digest.to_string())),
        (
            "value_bytes",
            nonnegative_safe_integer(&format!("{path}.value_bytes"), projected.value_bytes)?,
        ),
    ]))
}
