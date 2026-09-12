use std::collections::BTreeMap;

use amiss_wire::codec;
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use serde::Deserialize;

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

#[derive(Deserialize)]
struct ReportPayload {
    evaluation: Value,
}

#[derive(Deserialize)]
struct Evaluation {
    mode: String,
    event_kind: String,
    finality: String,
    materialization: String,
    skip_worktree_paths: u64,
    index_only_materialized_paths: u64,
    repository: Repository,
    base: Snapshot,
    candidate: Snapshot,
    #[serde(deserialize_with = "codec::nullable")]
    target_ref: Option<BranchRef>,
}

#[derive(Deserialize)]
struct Repository {
    host: String,
    owner: String,
    name: String,
}

#[derive(Deserialize)]
struct Snapshot {
    kind: String,
    object_format: ObjectFormat,
    commit_oid: Oid,
    tree_oid: Oid,
}

pub(crate) fn accepted_report(bytes: &[u8]) -> Result<AcceptedReport, ArtifactError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Err(ArtifactError::TooLarge);
    }
    let report = json::parse(bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let (payload, payload_digest, verdict) =
        amiss_wire::report::validate_envelope(&report).map_err(|_defect| ArtifactError::Corrupt)?;
    if verdict == amiss_wire::ExitClass::Failure {
        return Err(ArtifactError::Corrupt);
    }
    let payload_digest = Digest::from_wire(payload_digest).ok_or(ArtifactError::Corrupt)?;
    let payload: ReportPayload =
        codec::from_value("$.payload", payload).map_err(|_defect| ArtifactError::Corrupt)?;
    let evaluation: Evaluation = codec::from_value("$.payload.evaluation", &payload.evaluation)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    if evaluation.mode != "commit-pair"
        || evaluation.event_kind != "explicit-commit-pair"
        || evaluation.finality != "explicit-replay"
        || evaluation.materialization != "git-objects"
        || evaluation.skip_worktree_paths != 0
        || evaluation.index_only_materialized_paths != 0
        || evaluation.base.object_format != evaluation.candidate.object_format
    {
        return Err(ArtifactError::Corrupt);
    }
    let repository = RepositoryIdentity::new(
        evaluation.repository.host,
        evaluation.repository.owner,
        evaluation.repository.name,
    )
    .ok_or(ArtifactError::Corrupt)?;
    let mut identity: BTreeMap<String, Value> =
        serde_json::from_value(payload.evaluation).map_err(|_defect| ArtifactError::Corrupt)?;
    if identity.contains_key("schema") {
        return Err(ArtifactError::Corrupt);
    }
    identity.remove("evaluation_instant");
    identity.remove("trusted_time");
    identity.insert(
        "schema".to_owned(),
        codec::to_value(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN)
            .map_err(|_defect| ArtifactError::Corrupt)?,
    );
    let candidate_identity_digest =
        codec::digest(amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN, &identity)
            .map_err(|_defect| ArtifactError::Corrupt)?;
    Ok(AcceptedReport {
        report_digest: sha256(bytes),
        payload_digest,
        repository,
        target_ref: evaluation.target_ref,
        base: evaluation.base.accept()?,
        candidate: evaluation.candidate.accept()?,
        candidate_identity_digest,
    })
}

impl Snapshot {
    fn accept(self) -> Result<AcceptedSnapshot, ArtifactError> {
        if self.kind != "git-commit"
            || self.commit_oid.object_format() != self.object_format
            || self.tree_oid.object_format() != self.object_format
        {
            return Err(ArtifactError::Corrupt);
        }
        Ok(AcceptedSnapshot {
            commit: self.commit_oid,
            tree: self.tree_oid,
        })
    }
}

mod tests;
