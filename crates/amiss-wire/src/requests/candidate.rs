use serde::{Deserialize, Serialize};

use crate::assessment::Nullable;
use crate::digest::{Digest, hb};
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{CANDIDATE_IDENTITY_DOMAIN, EvaluationRequest, RequestMode, SnapshotMaterialization};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateIdentitySchema {
    #[serde(rename = "amiss/scanner-candidate-identity")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateEventKind {
    PullRequest,
    MergeGroup,
    DefaultBranchPush,
    ExplicitCommitPair,
    LocalIndex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateFinality {
    PrSyntheticMerge,
    MergeGroupCandidate,
    DefaultBranchUpdate,
    ExplicitReplay,
    LocalNonfinal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitSnapshotKind {
    #[serde(rename = "git-commit")]
    GitCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitSnapshotIdentity {
    pub kind: GitSnapshotKind,
    pub object_format: ObjectFormat,
    pub commit_oid: Oid,
    pub tree_oid: Oid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexSnapshotSchema {
    #[serde(rename = "amiss/scanner-snapshot")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexIdentityScope {
    #[serde(rename = "complete-logical-index")]
    CompleteLogicalIndex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum CandidateSnapshot {
    #[serde(rename = "git-commit")]
    GitCommit {
        object_format: ObjectFormat,
        commit_oid: Oid,
        tree_oid: Oid,
    },
    #[serde(rename = "index")]
    Index {
        snapshot_schema: IndexSnapshotSchema,
        identity_scope: IndexIdentityScope,
        base_object_format: ObjectFormat,
        base_commit_oid: Oid,
        index_projection_digest: Digest,
        entry_count: u64,
        snapshot_digest: Digest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateIdentity {
    pub schema: CandidateIdentitySchema,
    pub mode: RequestMode,
    pub event_kind: CandidateEventKind,
    pub finality: CandidateFinality,
    pub repository: Nullable<RepositoryIdentity>,
    pub candidate_ref: Nullable<BranchRef>,
    pub target_ref: Nullable<BranchRef>,
    pub default_branch_ref: Nullable<BranchRef>,
    pub base: GitSnapshotIdentity,
    pub candidate: CandidateSnapshot,
    pub materialization: SnapshotMaterialization,
    pub skip_worktree_paths: u64,
    pub index_only_materialized_paths: u64,
    pub forge: Nullable<ForgeDialect>,
}

/// Computes the commit-pair candidate identity carried by a complete report.
/// The tree IDs come from independent acquisition because the evaluation
/// request deliberately names only commits.
#[must_use]
pub fn commit_candidate_identity_digest(
    evaluation: &EvaluationRequest,
    base_tree: &Oid,
    candidate_tree: &Oid,
) -> Option<Digest> {
    evaluation.canonical_bytes().ok()?;
    let candidate_commit = match (evaluation.mode, evaluation.candidate_commit.as_ref()) {
        (RequestMode::CommitPair, Some(candidate)) => candidate.clone(),
        (RequestMode::CommitPair | RequestMode::Index, None | Some(_)) => return None,
    };
    let object_format = evaluation.object_format;
    let base_tree = Oid::new(object_format, base_tree.as_str().to_owned())?;
    let candidate_tree = Oid::new(object_format, candidate_tree.as_str().to_owned())?;
    let identity = CandidateIdentity {
        schema: CandidateIdentitySchema::Current,
        mode: RequestMode::CommitPair,
        event_kind: CandidateEventKind::ExplicitCommitPair,
        finality: CandidateFinality::ExplicitReplay,
        repository: evaluation
            .repository
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        candidate_ref: evaluation
            .candidate_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        target_ref: evaluation
            .target_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        default_branch_ref: evaluation
            .default_branch_ref
            .clone()
            .map_or(Nullable::Null, Nullable::Value),
        base: GitSnapshotIdentity {
            kind: GitSnapshotKind::GitCommit,
            object_format,
            commit_oid: evaluation.base_commit.clone(),
            tree_oid: base_tree,
        },
        candidate: CandidateSnapshot::GitCommit {
            object_format,
            commit_oid: candidate_commit,
            tree_oid: candidate_tree,
        },
        materialization: SnapshotMaterialization::GitObjects,
        skip_worktree_paths: 0,
        index_only_materialized_paths: 0,
        forge: evaluation.forge.map_or(Nullable::Null, Nullable::Value),
    };
    serde_json_canonicalizer::to_vec(&identity)
        .ok()
        .map(|canonical| hb(CANDIDATE_IDENTITY_DOMAIN, &canonical))
}
