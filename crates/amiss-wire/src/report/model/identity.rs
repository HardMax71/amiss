use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::model::{BranchRef, ForgeDialect, RepositoryIdentity, UtcInstant};
use crate::requests::{
    CandidateEventKind, CandidateFinality, CandidateIdentitySchema, RequestMode,
    SnapshotMaterialization,
};

use super::{BaseSnapshot, ResolvedEvaluation, Snapshot};

#[derive(Deserialize)]
#[serde(bound(deserialize = "E: Deserialize<'de>"))]
pub struct IdentityPayload<E = ResolvedEvaluation> {
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub evaluation: E,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Serialize)]
#[serde(remote = "ResolvedEvaluation")]
pub struct IdentityEvaluation {
    pub base: BaseSnapshot,
    pub candidate: Snapshot,
    pub candidate_ref: Option<BranchRef>,
    pub default_branch_ref: Option<BranchRef>,
    #[serde(skip_serializing)]
    pub evaluation_instant: Option<UtcInstant>,
    pub event_kind: CandidateEventKind,
    pub finality: CandidateFinality,
    pub forge: Option<ForgeDialect>,
    pub index_only_materialized_paths: u64,
    pub materialization: SnapshotMaterialization,
    pub mode: RequestMode,
    pub repository: Option<RepositoryIdentity>,
    pub skip_worktree_paths: u64,
    pub target_ref: Option<BranchRef>,
    #[serde(skip_serializing)]
    pub trusted_time: bool,
}

#[derive(Serialize)]
pub struct IdentityPreimage<'a> {
    #[serde(flatten, with = "IdentityEvaluation")]
    pub evaluation: &'a ResolvedEvaluation,
    pub schema: CandidateIdentitySchema,
}
