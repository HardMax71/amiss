use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity, UtcInstant};
use crate::requests::{
    CandidateEventKind, CandidateFinality, RequestMode, SnapshotMaterialization,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitSnapshotKind {
    #[serde(rename = "git-commit")]
    GitCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSnapshot {
    pub commit_oid: Oid,
    pub kind: GitSnapshotKind,
    pub object_format: ObjectFormat,
    pub tree_oid: Oid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyntheticSnapshotKind {
    #[serde(rename = "index")]
    Index,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyntheticSnapshotSchema {
    #[serde(rename = "amiss/scanner-snapshot")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyntheticIdentityScope {
    #[serde(rename = "complete-logical-index")]
    CompleteLogicalIndex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticSnapshot {
    pub base_commit_oid: Oid,
    pub base_object_format: ObjectFormat,
    pub entry_count: u64,
    pub identity_scope: SyntheticIdentityScope,
    pub index_projection_digest: Digest,
    pub kind: SyntheticSnapshotKind,
    pub snapshot_digest: Digest,
    pub snapshot_schema: SyntheticSnapshotSchema,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnavailableSnapshotKind {
    #[serde(rename = "unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotUnavailableReason {
    IndexInvalid,
    IndexUnmerged,
    IntentToAdd,
    MissingObject,
    NotEvaluated,
    NotSupplied,
    RepositoryUnavailable,
    ResourceLimit,
    SnapshotChanged,
    UnreadableObject,
    UnrepresentablePath,
    WrongObjectKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableSnapshot {
    pub kind: UnavailableSnapshotKind,
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
    pub reasons: Vec<SnapshotUnavailableReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BaseSnapshot {
    Git(GitSnapshot),
    Unavailable(UnavailableSnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Snapshot {
    Git(GitSnapshot),
    Synthetic(SyntheticSnapshot),
    Unavailable(UnavailableSnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEvaluation {
    pub base: BaseSnapshot,
    pub candidate: Snapshot,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_ref: Option<BranchRef>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub default_branch_ref: Option<BranchRef>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub evaluation_instant: Option<UtcInstant>,
    pub event_kind: CandidateEventKind,
    pub finality: CandidateFinality,
    #[serde(deserialize_with = "Option::deserialize")]
    pub forge: Option<ForgeDialect>,
    pub index_only_materialized_paths: u64,
    pub materialization: SnapshotMaterialization,
    pub mode: RequestMode,
    #[serde(deserialize_with = "Option::deserialize")]
    pub repository: Option<RepositoryIdentity>,
    pub skip_worktree_paths: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target_ref: Option<BranchRef>,
    pub trusted_time: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnavailableStatus {
    #[serde(rename = "unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationUnavailableReason {
    InvalidEvent,
    InvalidInvocation,
    InvalidProfile,
    RequestUnreadable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableEvaluation {
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
    pub reasons: Vec<EvaluationUnavailableReason>,
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Evaluation {
    Resolved(Box<ResolvedEvaluation>),
    Unavailable(UnavailableEvaluation),
}
