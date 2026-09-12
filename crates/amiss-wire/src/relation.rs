use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{Document, Envelope, Schema, sorted_roles};
use crate::controls::{ProjectionKind, ProjectionSource, compatible_source};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, RelationAssessment,
    RelationAssessmentEnvelope, RelationReason, RelationVerdict, assess, parse_assessment,
};

pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, RelationEvidence, RelationEvidenceEnvelope,
    RelationEvidenceSubject, RelationProjectedValue,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/relation-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/relation-plan-payload";
pub const RELATION_DOCUMENT_BYTES: u64 = 65_536;

pub type RelationPlanEnvelope = Envelope<RelationPlan>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationPlan {
    pub schema: Schema<Self>,
    pub report_payload_digest: Digest,
    pub relation: RelationIdentity,
    pub coordination: ArtifactId,
    pub trigger_role: ArtifactId,
    pub projection: ProjectionKind,
    #[garde(dive)]
    pub subjects: [RelationSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationIdentity {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct RelationSubject {
    pub role: ArtifactId,
    pub repository: RepositoryIdentity,
    pub target: BranchRef,
    pub object_format: ObjectFormat,
    #[garde(dive)]
    pub source: ProjectionSource,
    pub base: RelationSnapshot,
    pub candidate: RelationSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationSnapshot {
    #[serde(rename = "commit_oid")]
    pub commit: Oid,
    #[serde(rename = "tree_oid")]
    pub tree: Oid,
}

impl Document for RelationPlan {
    const PAYLOAD_SCHEMA: &'static str = PLAN_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = PLAN_ENVELOPE_SCHEMA;
    const LIMIT: u64 = RELATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        let [left, right] = &self.subjects;
        sorted_roles(root, &left.role, &right.role)?;
        if left.repository == right.repository
            || !self
                .subjects
                .iter()
                .any(|subject| subject.role == self.trigger_role)
        {
            return fail(root, ErrorKind::Inconsistent);
        }
        self.subjects
            .iter()
            .enumerate()
            .try_for_each(|(index, subject)| {
                subject.check(&format!("{root}.subjects[{index}]"), self.projection)
            })
    }
}

impl RelationSubject {
    fn check(&self, path: &str, projection: ProjectionKind) -> Result<(), Error> {
        let source = format!("{path}.source");
        self.source.check(&source)?;
        compatible_source(projection, &self.source, &source)?;
        [("base", &self.base), ("candidate", &self.candidate)]
            .into_iter()
            .try_for_each(|(name, snapshot)| {
                snapshot.check(&format!("{path}.{name}"), self.object_format)
            })
    }
}

impl RelationSnapshot {
    fn check(&self, path: &str, object_format: ObjectFormat) -> Result<(), Error> {
        [("commit_oid", &self.commit), ("tree_oid", &self.tree)]
            .into_iter()
            .find(|(_, oid)| oid.object_format() != object_format)
            .map_or(Ok(()), |(field, _)| {
                fail(&format!("{path}.{field}"), ErrorKind::InvalidValue)
            })
    }
}

/// Reads one bounded relation plan and verifies its constraints and digest.
///
/// # Errors
///
/// The document violates its shape, constraints, or payload digest binding.
pub fn parse_plan(bytes: &[u8]) -> Result<RelationPlanEnvelope, Error> {
    RelationPlanEnvelope::parse(bytes)
}

/// Seals a relation plan after validating its field laws.
///
/// # Errors
///
/// The plan violates its contract or exceeds the document byte ceiling.
pub fn plan(input: &RelationPlan) -> Result<crate::json::Value, Error> {
    crate::codec::seal_value(input)
}
