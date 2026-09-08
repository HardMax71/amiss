use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{ProjectionKind, ProjectionSource, check_projection_source};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj_serde, verified_json_digest};
use crate::json;
use crate::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, AssessmentEnvelopeSchema,
    AssessmentPayloadSchema, RelationAssessment, RelationAssessmentEnvelope, RelationReason,
    RelationVerdict, assess, parse_assessment,
};

pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, EvidenceEnvelopeSchema,
    EvidencePayloadSchema, RelationEvidence, RelationEvidenceEnvelope, RelationEvidenceSubject,
    RelationProjectedValue, RelationProjectionSlot, evidence, parse_evidence,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/relation-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/relation-plan-payload";
pub const RELATION_DOCUMENT_BYTES: u64 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationPlanEnvelope {
    pub schema: PlanEnvelopeSchema,
    pub payload: RelationPlan,
    pub payload_digest: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PlanEnvelopeSchema {
    #[strum(serialize = "amiss/relation-plan-envelope")]
    Current,
}

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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PlanPayloadSchema {
    #[strum(serialize = "amiss/relation-plan-payload")]
    Current,
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

/// Parses one closed, digest-bound cross-repository relation plan.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity, selector, branch, or Git object, unsorted subjects, inconsistent
/// object formats, or a payload digest mismatch.
pub fn parse_plan(bytes: &[u8]) -> Result<RelationPlanEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: RelationPlanEnvelope = serde_json::from_slice(bytes)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    verified_json_digest(PLAN_ENVELOPE_SCHEMA, bytes, &document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if plan_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Binds one owned cross-repository relation plan to its validated payload digest.
///
/// The result stays typed; output callers enforce byte limits with [`crate::write_json`].
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_plan`] enforces.
pub fn plan(input: RelationPlan) -> Result<RelationPlanEnvelope, Error> {
    let payload_digest = plan_payload_digest(&input)?;
    Ok(RelationPlanEnvelope {
        schema: PlanEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    })
}

/// Validates a complete relation plan and hashes its canonical payload without encoding an artifact.
///
/// # Errors
///
/// Fails on invalid subjects, identities, projection sources or inconsistent Git object formats.
pub fn plan_payload_digest(input: &RelationPlan) -> Result<Digest, Error> {
    validate_plan(input)?;
    hj_serde(PLAN_PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(input, &mut writer)
    })
    .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
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
