use serde::{Deserialize, Serialize};

use crate::codec::{self, nullable};
use crate::controls::Profile;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{
    CANDIDATE_IDENTITY_DOMAIN, EVALUATION_REQUEST_SCHEMA, RequestMode, decode_request,
    request_bytes,
};

/// The run-identity request: profile, mode, and the exact snapshot
/// identities to evaluate. The candidate commit is null exactly when the
/// mode is `index`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvaluationRequest {
    pub profile: Profile,
    pub mode: RequestMode,
    pub object_format: ObjectFormat,
    pub repository: Option<RepositoryIdentity>,
    pub forge: Option<ForgeDialect>,
    pub candidate_ref: Option<BranchRef>,
    pub target_ref: Option<BranchRef>,
    pub default_branch_ref: Option<BranchRef>,
    #[serde(rename = "base_commit_oid")]
    pub base_commit: Oid,
    #[serde(rename = "candidate_commit_oid")]
    pub candidate_commit: Option<Oid>,
}

impl EvaluationRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid
    /// grammar values, and a candidate commit inconsistent with the mode.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let document: EvaluationDocument = decode_request(bytes)?;
        if document.schema != EVALUATION_REQUEST_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        let request = Self {
            profile: document.profile,
            mode: document.mode,
            object_format: document.object_format,
            repository: document.repository,
            forge: document.forge,
            candidate_ref: document.candidate_ref,
            target_ref: document.target_ref,
            default_branch_ref: document.default_branch_ref,
            base_commit: document.base_commit_oid,
            candidate_commit: document.candidate_commit_oid,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.base_commit.object_format() != self.object_format {
            return fail("$.base_commit_oid", ErrorKind::InvalidValue);
        }
        if self
            .candidate_commit
            .as_ref()
            .is_some_and(|oid| oid.object_format() != self.object_format)
        {
            return fail("$.candidate_commit_oid", ErrorKind::InvalidValue);
        }
        let consistent = match self.mode {
            RequestMode::CommitPair => self.candidate_commit.is_some(),
            RequestMode::Index => self.candidate_commit.is_none(),
        };
        if !consistent {
            return fail("$.candidate_commit_oid", ErrorKind::Inconsistent);
        }
        let identity_fields = [
            self.repository.is_some(),
            self.candidate_ref.is_some(),
            self.target_ref.is_some(),
            self.default_branch_ref.is_some(),
        ];
        if (!identity_fields.iter().all(|present| *present)
            && identity_fields.iter().any(|present| *present))
            || self.forge.is_some() && self.repository.is_none()
            || matches!(
                self.forge,
                Some(
                    ForgeDialect::Github
                        | ForgeDialect::Gitea
                        | ForgeDialect::BitbucketCloud
                        | ForgeDialect::BitbucketDataCenter
                )
            ) && self
                .repository
                .as_ref()
                .is_some_and(|identity| identity.owner().contains('/'))
        {
            return fail("$.forge", ErrorKind::Inconsistent);
        }
        Ok(())
    }

    /// Builds an explicit-commit evaluation with no forge identity. Callers
    /// may then fill the public identity fields before serialization.
    #[must_use]
    pub fn commit_pair(
        profile: Profile,
        object_format: ObjectFormat,
        base_commit: Oid,
        candidate_commit: Oid,
    ) -> Self {
        Self::without_identity(profile, object_format, base_commit, Some(candidate_commit))
    }

    /// Builds a staged-index evaluation with no forge identity.
    #[must_use]
    pub fn index(profile: Profile, object_format: ObjectFormat, base_commit: Oid) -> Self {
        Self::without_identity(profile, object_format, base_commit, None)
    }

    fn without_identity(
        profile: Profile,
        object_format: ObjectFormat,
        base_commit: Oid,
        candidate_commit: Option<Oid>,
    ) -> Self {
        Self {
            profile,
            mode: if candidate_commit.is_some() {
                RequestMode::CommitPair
            } else {
                RequestMode::Index
            },
            object_format,
            repository: None,
            forge: None,
            candidate_ref: None,
            target_ref: None,
            default_branch_ref: None,
            base_commit,
            candidate_commit,
        }
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        request_bytes(&EvaluationOutput {
            schema: EVALUATION_REQUEST_SCHEMA,
            request: self,
        })
    }
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
    evaluation.validate().ok()?;
    let candidate_commit = match (evaluation.mode, evaluation.candidate_commit.as_ref()) {
        (RequestMode::CommitPair, Some(candidate)) => candidate,
        (RequestMode::CommitPair | RequestMode::Index, None | Some(_)) => return None,
    };
    let format = evaluation.object_format;
    if base_tree.object_format() != format || candidate_tree.object_format() != format {
        return None;
    }
    codec::digest(
        CANDIDATE_IDENTITY_DOMAIN,
        &CandidateIdentity {
            schema: CANDIDATE_IDENTITY_DOMAIN,
            mode: "commit-pair",
            event_kind: "explicit-commit-pair",
            finality: "explicit-replay",
            repository: evaluation.repository.as_ref(),
            candidate_ref: evaluation.candidate_ref.as_ref(),
            target_ref: evaluation.target_ref.as_ref(),
            default_branch_ref: evaluation.default_branch_ref.as_ref(),
            base: CommitSnapshot {
                kind: "git-commit",
                object_format: format,
                commit_oid: &evaluation.base_commit,
                tree_oid: base_tree,
            },
            candidate: CommitSnapshot {
                kind: "git-commit",
                object_format: format,
                commit_oid: candidate_commit,
                tree_oid: candidate_tree,
            },
            materialization: "git-objects",
            skip_worktree_paths: 0,
            index_only_materialized_paths: 0,
            forge: evaluation.forge,
        },
    )
    .ok()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDocument {
    schema: String,
    profile: Profile,
    mode: RequestMode,
    object_format: ObjectFormat,
    #[serde(deserialize_with = "nullable")]
    repository: Option<RepositoryIdentity>,
    #[serde(deserialize_with = "nullable")]
    forge: Option<ForgeDialect>,
    #[serde(deserialize_with = "nullable")]
    candidate_ref: Option<BranchRef>,
    #[serde(deserialize_with = "nullable")]
    target_ref: Option<BranchRef>,
    #[serde(deserialize_with = "nullable")]
    default_branch_ref: Option<BranchRef>,
    base_commit_oid: Oid,
    #[serde(deserialize_with = "nullable")]
    candidate_commit_oid: Option<Oid>,
}

#[derive(Serialize)]
struct EvaluationOutput<'a> {
    schema: &'static str,
    #[serde(flatten)]
    request: &'a EvaluationRequest,
}

#[derive(Serialize)]
struct CandidateIdentity<'a> {
    schema: &'static str,
    mode: &'static str,
    event_kind: &'static str,
    finality: &'static str,
    repository: Option<&'a RepositoryIdentity>,
    candidate_ref: Option<&'a BranchRef>,
    target_ref: Option<&'a BranchRef>,
    default_branch_ref: Option<&'a BranchRef>,
    base: CommitSnapshot<'a>,
    candidate: CommitSnapshot<'a>,
    materialization: &'static str,
    skip_worktree_paths: u64,
    index_only_materialized_paths: u64,
    forge: Option<ForgeDialect>,
}

#[derive(Serialize)]
struct CommitSnapshot<'a> {
    kind: &'static str,
    object_format: ObjectFormat,
    commit_oid: &'a Oid,
    tree_oid: &'a Oid,
}
