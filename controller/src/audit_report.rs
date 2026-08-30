use amiss_wire::digest::{Digest, hj, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

use crate::ArtifactError;

pub(crate) struct AcceptedReport {
    pub(crate) report_digest: Digest,
    pub(crate) payload_digest: Digest,
    pub(crate) repository: RepositoryIdentity,
    pub(crate) target_ref: Option<BranchRef>,
    pub(crate) base: AcceptedSnapshot,
    pub(crate) candidate: AcceptedSnapshot,
    pub(crate) candidate_identity_digest: Digest,
}

pub(crate) struct AcceptedSnapshot {
    pub(crate) commit: Oid,
    pub(crate) tree: Oid,
}

pub(crate) fn accepted_report(bytes: &[u8]) -> Result<AcceptedReport, ArtifactError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Err(ArtifactError::TooLarge);
    }
    let report = json::parse(bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let (_, payload_digest, verdict) =
        amiss_wire::report::validate_envelope(&report).map_err(|_defect| ArtifactError::Corrupt)?;
    if verdict == amiss_wire::ExitClass::Failure {
        return Err(ArtifactError::Corrupt);
    }
    let payload_digest = Digest::from_wire(payload_digest).ok_or(ArtifactError::Corrupt)?;
    let Value::Object(envelope) = report else {
        return Err(ArtifactError::Corrupt);
    };
    let payload = envelope
        .into_vec()
        .into_iter()
        .find_map(|(key, value)| (key == "payload").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    let Value::Object(payload) = payload else {
        return Err(ArtifactError::Corrupt);
    };
    let evaluation = payload
        .into_vec()
        .into_iter()
        .find_map(|(key, value)| (key == "evaluation").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    if evaluation.text("mode") != Some("commit-pair")
        || evaluation.text("event_kind") != Some("explicit-commit-pair")
        || evaluation.text("finality") != Some("explicit-replay")
        || evaluation.text("materialization") != Some("git-objects")
        || evaluation.member("skip_worktree_paths") != Some(&Value::Integer(0))
        || evaluation.member("index_only_materialized_paths") != Some(&Value::Integer(0))
        || evaluation.member("schema").is_some()
    {
        return Err(ArtifactError::Corrupt);
    }
    let repository = evaluation
        .member("repository")
        .and_then(|repository| {
            RepositoryIdentity::new(
                repository.text("host")?.to_owned(),
                repository.text("owner")?.to_owned(),
                repository.text("name")?.to_owned(),
            )
        })
        .ok_or(ArtifactError::Corrupt)?;
    let (base_format, base) = snapshot(evaluation.member("base").ok_or(ArtifactError::Corrupt)?)
        .ok_or(ArtifactError::Corrupt)?;
    let (candidate_format, candidate) = snapshot(
        evaluation
            .member("candidate")
            .ok_or(ArtifactError::Corrupt)?,
    )
    .ok_or(ArtifactError::Corrupt)?;
    if base_format != candidate_format {
        return Err(ArtifactError::Corrupt);
    }
    let target_ref = match evaluation.member("target_ref") {
        Some(Value::String(value)) => {
            Some(BranchRef::new(value.to_string()).ok_or(ArtifactError::Corrupt)?)
        }
        Some(Value::Null) => None,
        Some(Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_)) | None => {
            return Err(ArtifactError::Corrupt);
        }
    };
    let Value::Object(members) = evaluation else {
        return Err(ArtifactError::Corrupt);
    };
    let mut identity = vec![(
        "schema".to_owned(),
        Value::string(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    )];
    identity.extend(
        members
            .into_vec()
            .into_iter()
            .filter(|(key, _value)| !matches!(key.as_str(), "evaluation_instant" | "trusted_time")),
    );
    let identity = Value::object(identity);
    Ok(AcceptedReport {
        report_digest: sha256(bytes),
        payload_digest,
        repository,
        target_ref,
        base,
        candidate,
        candidate_identity_digest: hj(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN, &identity),
    })
}

fn snapshot(value: &Value) -> Option<(ObjectFormat, AcceptedSnapshot)> {
    if value.text("kind") != Some("git-commit") {
        return None;
    }
    let object_format = value.text("object_format")?.parse().ok()?;
    Some((
        object_format,
        AcceptedSnapshot {
            commit: Oid::new(object_format, value.text("commit_oid")?.to_owned())?,
            tree: Oid::new(object_format, value.text("tree_oid")?.to_owned())?,
        },
    ))
}
