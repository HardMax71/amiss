use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::model::ArtifactId;
use crate::model::Digest;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct RelationEvidenceEnvelope<T = RelationEvidence> {
    pub schema: EvidenceEnvelopeSchema,
    pub payload: T,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationEvidence {
    pub schema: EvidencePayloadSchema,
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidenceEnvelopeSchema {
    #[strum(serialize = "amiss/relation-evidence-envelope")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidencePayloadSchema {
    #[strum(serialize = "amiss/relation-evidence-payload")]
    Current,
}

/// Parses one closed, digest-bound set of four relation projections.
///
/// # Errors
///
/// Fails on oversized or malformed JSON, an unknown field, an invalid
/// identity or digest, reordered or repeated subject roles, an unsafe byte
/// count, or a payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<RelationEvidenceEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let document: RelationEvidenceEnvelope = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;

    if evidence_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for four relation projection slots.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar
/// [`parse_evidence`] enforces or the encoded document exceeds its byte
/// ceiling.
pub fn evidence(input: &RelationEvidence) -> Result<Vec<u8>, Error> {
    let payload_digest = evidence_payload_digest(input)?;
    let document = RelationEvidenceEnvelope {
        schema: EvidenceEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    Ok(canonical)
}

pub(super) fn evidence_payload_digest(input: &RelationEvidence) -> Result<Digest, Error> {
    validate(input)?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| {
            Digest::from(
                sha2::Sha256::new_with_prefix(EVIDENCE_PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(&canonical)
                    .finalize()
                    .0,
            )
        })
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
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
                && projected.value_bytes > js_int::MAX_SAFE_UINT
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
