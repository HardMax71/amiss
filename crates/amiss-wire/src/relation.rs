use crate::controls::value::{object, repository, text};
use crate::controls::{
    ProjectionKind, ProjectionSource, decode_checked_projection_source, decode_enum,
    decode_repository, projection_source_value,
};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/relation-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/relation-plan-payload";
pub const RELATION_DOCUMENT_BYTES: u64 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationPlanEnvelope {
    pub payload: RelationPlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationPlan {
    pub report_payload_digest: Digest,
    pub relation: RelationIdentity,
    pub trigger_role: ArtifactId,
    pub projection: ProjectionKind,
    pub subjects: [RelationSubject; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationIdentity {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubject {
    pub role: ArtifactId,
    pub repository: RepositoryIdentity,
    pub target: BranchRef,
    pub source: ProjectionSource,
    pub base: RelationSnapshot,
    pub candidate: RelationSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSnapshot {
    pub commit: Oid,
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
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        PLAN_ENVELOPE_SCHEMA,
        PLAN_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
        decode_plan,
    )?;
    Ok(RelationPlanEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for one cross-repository relation plan.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_plan`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn plan(input: &RelationPlan) -> Result<Value, Error> {
    let payload = plan_value(input);
    let _validated = decode_plan("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        PLAN_ENVELOPE_SCHEMA,
        PLAN_PAYLOAD_SCHEMA,
        RELATION_DOCUMENT_BYTES,
    )
}

fn decode_plan(path: &str, value: Value) -> Result<RelationPlan, Error> {
    let mut plan = Obj::new(path, value)?;
    plan.required("schema", |path, value| {
        de::const_str(path, value, PLAN_PAYLOAD_SCHEMA)
    })?;
    let report_payload_digest = plan.required("report_payload_digest", de::digest)?;
    let relation = plan.required("relation", |path, value| {
        let mut relation = Obj::new(path, value)?;
        let identity = relation.required("identity", |path, value| {
            ArtifactId::new(de::string(path, value)?)
                .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
        })?;
        let context_digest = relation.required("context_digest", de::digest)?;
        relation.finish()?;
        Ok(RelationIdentity {
            identity,
            context_digest,
        })
    })?;
    let trigger_role = plan.required("trigger_role", |path, value| {
        ArtifactId::new(de::string(path, value)?)
            .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
    })?;
    let projection = plan.required("projection", decode_enum)?;
    let subjects_path = plan.field("subjects");
    let subjects: [RelationSubject; 2] = de::array(&subjects_path, plan.take("subjects")?)?
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            decode_subject(&format!("{subjects_path}[{index}]"), value, projection)
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_subjects: Vec<RelationSubject>| {
            Error::new(&subjects_path, ErrorKind::InvalidValue)
        })?;
    plan.finish()?;

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
    if left.repository == right.repository
        || !subjects.iter().any(|subject| subject.role == trigger_role)
    {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(RelationPlan {
        report_payload_digest,
        relation,
        trigger_role,
        projection,
        subjects,
    })
}

fn decode_subject(
    path: &str,
    value: Value,
    projection: ProjectionKind,
) -> Result<RelationSubject, Error> {
    let mut subject = Obj::new(path, value)?;
    let role = subject.required("role", |path, value| {
        ArtifactId::new(de::string(path, value)?)
            .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
    })?;
    let repository = subject.required("repository", decode_repository)?;
    let target = subject.required("target", |path, value| {
        BranchRef::new(de::string(path, value)?)
            .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
    })?;
    let object_format = subject.required("object_format", decode_enum)?;
    let source = subject.required("source", |path, value| {
        decode_checked_projection_source(path, value, projection)
    })?;
    let base = subject.required("base", |path, value| {
        decode_snapshot(path, value, object_format)
    })?;
    let candidate = subject.required("candidate", |path, value| {
        decode_snapshot(path, value, object_format)
    })?;
    subject.finish()?;
    Ok(RelationSubject {
        role,
        repository,
        target,
        source,
        base,
        candidate,
    })
}

fn decode_snapshot(
    path: &str,
    value: Value,
    object_format: ObjectFormat,
) -> Result<RelationSnapshot, Error> {
    let mut snapshot = Obj::new(path, value)?;
    let commit_path = snapshot.field("commit_oid");
    let commit = Oid::new(
        object_format,
        de::string(&commit_path, snapshot.take("commit_oid")?)?,
    )
    .ok_or_else(|| Error::new(&commit_path, ErrorKind::InvalidValue))?;
    let tree_path = snapshot.field("tree_oid");
    let tree = Oid::new(
        object_format,
        de::string(&tree_path, snapshot.take("tree_oid")?)?,
    )
    .ok_or_else(|| Error::new(&tree_path, ErrorKind::InvalidValue))?;
    snapshot.finish()?;
    Ok(RelationSnapshot { commit, tree })
}

fn plan_value(plan: &RelationPlan) -> Value {
    object(vec![
        ("schema", text(PLAN_PAYLOAD_SCHEMA)),
        (
            "report_payload_digest",
            text(&plan.report_payload_digest.to_string()),
        ),
        (
            "relation",
            object(vec![
                ("identity", text(plan.relation.identity.as_str())),
                (
                    "context_digest",
                    text(&plan.relation.context_digest.to_string()),
                ),
            ]),
        ),
        ("trigger_role", text(plan.trigger_role.as_str())),
        ("projection", text(plan.projection.as_ref())),
        (
            "subjects",
            Value::array(plan.subjects.iter().map(subject_value).collect()),
        ),
    ])
}

fn subject_value(subject: &RelationSubject) -> Value {
    object(vec![
        ("role", text(subject.role.as_str())),
        ("repository", repository(&subject.repository)),
        ("target", text(subject.target.as_str())),
        (
            "object_format",
            text(subject.base.commit.object_format().as_ref()),
        ),
        ("source", projection_source_value(&subject.source)),
        ("base", snapshot_value(&subject.base)),
        ("candidate", snapshot_value(&subject.candidate)),
    ])
}

fn snapshot_value(snapshot: &RelationSnapshot) -> Value {
    object(vec![
        ("commit_oid", text(snapshot.commit.as_str())),
        ("tree_oid", text(snapshot.tree.as_str())),
    ])
}
