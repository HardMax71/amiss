use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{Error, ErrorKind, fail};
use crate::envelope::Payload;
use crate::model::ArtifactId;
use crate::model::Digest;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

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
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum EvidenceEnvelopeSchema {
    #[default]
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

impl Payload for RelationEvidence {
    type Schema = EvidenceEnvelopeSchema;
    type Defect = Error;
    const DOMAIN: &'static str = EVIDENCE_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = RELATION_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate(self)
    }
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
