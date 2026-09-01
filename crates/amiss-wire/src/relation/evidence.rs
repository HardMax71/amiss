use serde::{Deserialize, Serialize};

use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json::{self, Value};
use crate::model::ArtifactId;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationEvidenceEnvelope {
    pub payload: RelationEvidence,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationEvidence {
    pub plan_payload_digest: Digest,
    pub subjects: [RelationEvidenceSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationEvidenceSubject {
    pub role: ArtifactId,
    pub base: RelationProjectionSlot,
    pub candidate: RelationProjectionSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RelationProjectionSlot {
    Projected(RelationProjectedValue),
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationProjectedValue {
    pub value_digest: Digest,
    pub value_bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum EvidenceEnvelope<T> {
    #[serde(rename = "amiss/relation-evidence-envelope")]
    Current { payload: T, payload_digest: Digest },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum EvidencePayload<T> {
    #[serde(rename = "amiss/relation-evidence-payload")]
    Current(T),
}

/// Parses one closed, digest-bound set of four relation projections.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or digest, reordered or repeated subject roles, an unsafe byte
/// count, or a payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<RelationEvidenceEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: EvidenceEnvelope<EvidencePayload<RelationEvidence>> =
        serde_json::from_slice(bytes)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let EvidenceEnvelope::Current {
        payload,
        payload_digest,
    } = document;
    let EvidencePayload::Current(payload) = payload;
    validate(&payload)?;
    let canonical = serde_json_canonicalizer::to_vec(&EvidencePayload::Current(&payload))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
    if hb(EVIDENCE_PAYLOAD_SCHEMA, &canonical) != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
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
    validate(input)?;
    let payload = EvidencePayload::Current(input);
    let canonical_payload = serde_json_canonicalizer::to_vec(&payload)
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))?;
    let document = EvidenceEnvelope::Current {
        payload,
        payload_digest: hb(EVIDENCE_PAYLOAD_SCHEMA, &canonical_payload),
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn validate(evidence: &RelationEvidence) -> Result<(), Error> {
    let [left, right] = &evidence.subjects;
    if left.role == right.role {
        return fail("$.payload.subjects", ErrorKind::DuplicateMember);
    }
    if left.role > right.role {
        return fail("$.payload.subjects", ErrorKind::UnsortedSet);
    }
    for (index, subject) in evidence.subjects.iter().enumerate() {
        for (field, slot) in [("base", subject.base), ("candidate", subject.candidate)] {
            if let RelationProjectionSlot::Projected(projected) = slot
                && projected.value_bytes > json::MAX_SAFE_INTEGER.unsigned_abs()
            {
                return fail(
                    &format!("$.payload.subjects[{index}].{field}.value_bytes"),
                    ErrorKind::InvalidValue,
                );
            }
        }
    }
    Ok(())
}
