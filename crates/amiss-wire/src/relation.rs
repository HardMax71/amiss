use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{ProjectionKind, ProjectionSource, check_projection_source};
use crate::de::{Error, ErrorKind, fail};
use crate::envelope::Payload;
use crate::model::Digest;
use crate::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, AssessmentEnvelopeSchema,
    AssessmentPayloadSchema, RelationAssessment, RelationReason, RelationVerdict, assess,
};

pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, EvidenceEnvelopeSchema,
    EvidencePayloadSchema, RelationEvidence, RelationEvidenceSubject, RelationProjectedValue,
    RelationProjectionSlot,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/relation-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/relation-plan-payload";
pub const RELATION_DOCUMENT_BYTES: u64 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationPlan {
    pub schema: PlanPayloadSchema,
    pub report_payload_digest: Digest,
    pub relation: RelationIdentity,
    pub coordination: ArtifactId,
    pub trigger_role: ArtifactId,
    pub projection: ProjectionKind,
    pub subjects: [RelationSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationIdentity {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationSubject {
    pub role: ArtifactId,
    pub repository: RepositoryIdentity,
    pub target: BranchRef,
    pub object_format: ObjectFormat,
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
pub enum PlanEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/relation-plan-envelope")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PlanPayloadSchema {
    #[strum(serialize = "amiss/relation-plan-payload")]
    Current,
}

impl Payload for RelationPlan {
    type Schema = PlanEnvelopeSchema;
    const DOMAIN: &'static str = PLAN_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = RELATION_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate_plan(self)
    }
}

fn validate_plan(plan: &RelationPlan) -> Result<(), Error> {
    let [left, right] = &plan.subjects;
    if left.role >= right.role {
        return fail(
            "$.payload.subjects",
            if left.role == right.role {
                ErrorKind::DuplicateMember
            } else {
                ErrorKind::UnsortedSet
            },
        );
    }
    if left.repository == right.repository
        || !plan
            .subjects
            .iter()
            .any(|subject| subject.role == plan.trigger_role)
    {
        return fail("$.payload", ErrorKind::Inconsistent);
    }
    for (index, subject) in plan.subjects.iter().enumerate() {
        if RepositoryIdentity::new(
            subject.repository.host().to_owned(),
            subject.repository.owner().to_owned(),
            subject.repository.name().to_owned(),
        )
        .as_ref()
            != Some(&subject.repository)
        {
            return fail(
                &format!("$.payload.subjects[{index}].repository"),
                ErrorKind::InvalidValue,
            );
        }
        if let Err(error) = check_projection_source(plan.projection, &subject.source) {
            return fail(&format!("$.payload.subjects[{index}].source"), error.kind);
        }
        for (snapshot_name, snapshot) in
            [("base", &subject.base), ("candidate", &subject.candidate)]
        {
            for (oid_name, oid) in [
                ("commit_oid", &snapshot.commit),
                ("tree_oid", &snapshot.tree),
            ] {
                if oid.object_format() != subject.object_format {
                    return fail(
                        &format!("$.payload.subjects[{index}].{snapshot_name}.{oid_name}"),
                        ErrorKind::InvalidValue,
                    );
                }
            }
        }
    }
    Ok(())
}
