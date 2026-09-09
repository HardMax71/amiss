use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hj_serde};
use crate::json;
use crate::model::ArtifactId;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationEvidenceEnvelope {
    pub schema: EvidenceEnvelopeSchema,
    pub payload: RelationEvidence,
    pub payload_digest: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidenceEnvelopeSchema {
    #[strum(serialize = "amiss/relation-evidence-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationEvidence {
    pub schema: EvidencePayloadSchema,
    pub plan_payload_digest: Digest,
    pub subjects: [RelationEvidenceSubject; 2],
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidencePayloadSchema {
    #[strum(serialize = "amiss/relation-evidence-payload")]
    Current,
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
    #[serde(with = "As::<TryFromInto<UInt>>")]
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
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let (document, _digest): (RelationEvidenceEnvelope, _) =
        de::deserialize_json(bytes, EVIDENCE_ENVELOPE_SCHEMA)?;
    if evidence_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Binds four owned relation projection slots to their validated payload digest.
///
/// The result stays typed; output callers enforce byte limits with [`crate::write_json`].
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_evidence`] enforces.
pub fn evidence(input: RelationEvidence) -> Result<RelationEvidenceEnvelope, Error> {
    let payload_digest = evidence_payload_digest(&input)?;
    Ok(RelationEvidenceEnvelope {
        schema: EvidenceEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    })
}

pub(super) fn evidence_payload_digest(input: &RelationEvidence) -> Result<Digest, Error> {
    validate(input)?;
    hj_serde(EVIDENCE_PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(input, &mut writer)
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
