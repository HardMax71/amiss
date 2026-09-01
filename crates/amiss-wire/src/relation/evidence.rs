use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{Document, Envelope, MAX_SAFE_INTEGER, Schema, sorted_roles};
use crate::de::Error;
use crate::digest::Digest;
use crate::model::ArtifactId;

use super::RELATION_DOCUMENT_BYTES;

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/relation-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/relation-evidence-payload";

pub type RelationEvidenceEnvelope = Envelope<RelationEvidence>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationEvidence {
    pub schema: Schema<Self>,
    pub plan_payload_digest: Digest,
    #[garde(dive)]
    pub subjects: [RelationEvidenceSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationEvidenceSubject {
    pub role: ArtifactId,
    #[serde(deserialize_with = "crate::codec::nullable")]
    #[garde(dive)]
    pub base: Option<RelationProjectedValue>,
    #[serde(deserialize_with = "crate::codec::nullable")]
    #[garde(dive)]
    pub candidate: Option<RelationProjectedValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationProjectedValue {
    pub value_digest: Digest,
    #[garde(range(max = MAX_SAFE_INTEGER))]
    pub value_bytes: u64,
}

impl Document for RelationEvidence {
    const PAYLOAD_SCHEMA: &'static str = EVIDENCE_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = EVIDENCE_ENVELOPE_SCHEMA;
    const LIMIT: u64 = RELATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        let [left, right] = &self.subjects;
        sorted_roles(root, &left.role, &right.role)
    }
}
