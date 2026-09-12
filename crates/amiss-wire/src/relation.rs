use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::Digest as _;
use strum::{Display, EnumString};

use crate::controls::{ProjectionKind, ProjectionSource, check_projection_source};
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;
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
#[serde(remote = "Self", bound(deserialize = "T: Deserialize<'de>"))]
pub struct RelationPlanEnvelope<T = RelationPlan> {
    pub schema: PlanEnvelopeSchema,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub payload: T,
    pub payload_digest: Digest,
}

impl<T: Serialize> Serialize for RelationPlanEnvelope<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for RelationPlanEnvelope<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

impl RelationPlanEnvelope {
    /// Checks the plan's domain laws, payload binding, and encoded document ceiling.
    ///
    /// # Errors
    /// Refuses invalid subjects or projections, oversized output, and a mismatched payload digest.
    pub fn validate(&self) -> Result<(), Error> {
        let digest = plan_payload_digest(&self.payload)?;
        let mut measured = countio::Counter::new(std::io::sink());
        serde_json_canonicalizer::to_writer(self, &mut measured)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        if u64::try_from(measured.writer_bytes()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
            return fail("$", ErrorKind::LimitExceeded);
        }
        if digest != self.payload_digest {
            return fail("$.payload_digest", ErrorKind::DigestMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct RelationPlan {
    pub schema: PlanPayloadSchema,
    pub report_payload_digest: Digest,
    pub relation: RelationIdentity,
    pub coordination: ArtifactId,
    pub trigger_role: ArtifactId,
    pub projection: ProjectionKind,
    pub subjects: [RelationSubject; 2],
}

impl Serialize for RelationPlan {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RelationPlan {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct RelationIdentity {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

impl Serialize for RelationIdentity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RelationIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct RelationSubject {
    pub role: ArtifactId,
    pub repository: RepositoryIdentity,
    pub target: BranchRef,
    pub object_format: ObjectFormat,
    pub source: ProjectionSource,
    pub base: RelationSnapshot,
    pub candidate: RelationSnapshot,
}

impl Serialize for RelationSubject {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RelationSubject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(remote = "Self")]
pub struct RelationSnapshot {
    #[serde(rename = "commit_oid")]
    pub commit: Oid,
    #[serde(rename = "tree_oid")]
    pub tree: Oid,
}

impl Serialize for RelationSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RelationSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PlanEnvelopeSchema {
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
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let document: RelationPlanEnvelope = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if plan_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for one cross-repository relation plan.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_plan`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn plan(input: &RelationPlan) -> Result<Vec<u8>, Error> {
    let payload_digest = plan_payload_digest(input)?;
    let document = RelationPlanEnvelope {
        schema: PlanEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    de::JsonProfile::validate(&canonical)?;
    Ok(canonical)
}

pub(super) fn plan_payload_digest(input: &RelationPlan) -> Result<Digest, Error> {
    validate_plan(input)?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| {
            Digest::from(
                sha2::Sha256::new_with_prefix(PLAN_PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(&canonical)
                    .finalize()
                    .0,
            )
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
