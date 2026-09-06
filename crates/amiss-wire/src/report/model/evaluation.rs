use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::model::{BranchRef, ForgeDialect, RepositoryIdentity, UtcInstant};
use crate::requests::{
    CandidateEventKind, CandidateFinality, CandidateSnapshot, RequestMode, SnapshotMaterialization,
};

pub use crate::requests::{
    GitSnapshotIdentity as GitSnapshot, GitSnapshotKind,
    IndexIdentityScope as SyntheticIdentityScope, IndexSnapshotIdentity as SyntheticSnapshot,
    IndexSnapshotKind as SyntheticSnapshotKind, IndexSnapshotSchema as SyntheticSnapshotSchema,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnavailableSnapshotKind {
    #[serde(rename = "unavailable")]
    Unavailable,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr, strum::EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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
    pub reasons: Vec<SnapshotUnavailableReason>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
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
    Available(CandidateSnapshot),
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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
)]
pub enum UnavailableStatus {
    #[strum(serialize = "unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum EvaluationUnavailableReason {
    InvalidEvent,
    InvalidInvocation,
    InvalidProfile,
    RequestUnreadable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableEvaluation {
    pub reasons: Vec<EvaluationUnavailableReason>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub request_digest: Option<Digest>,
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Evaluation {
    Resolved(Box<ResolvedEvaluation>),
    Unavailable(UnavailableEvaluation),
}
