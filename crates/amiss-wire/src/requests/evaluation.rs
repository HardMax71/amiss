use crate::controls::value::{object, repository, text};
use crate::controls::{Profile, decode_enum, decode_repository, root};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{CANDIDATE_IDENTITY_DOMAIN, EVALUATION_REQUEST_SCHEMA, RequestMode, checked_canonical};

/// The run-identity request: profile, mode, and the exact snapshot
/// identities to evaluate. The candidate commit is null exactly when the
/// mode is `index`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationRequest {
    pub profile: Profile,
    pub mode: RequestMode,
    pub object_format: ObjectFormat,
    pub repository: Option<RepositoryIdentity>,
    pub forge: Option<ForgeDialect>,
    pub candidate_ref: Option<BranchRef>,
    pub target_ref: Option<BranchRef>,
    pub default_branch_ref: Option<BranchRef>,
    pub base_commit: Oid,
    pub candidate_commit: Option<Oid>,
}

impl EvaluationRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid
    /// grammar values, and a candidate commit inconsistent with the mode.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, EVALUATION_REQUEST_SCHEMA)
        })?;
        let profile = obj.required("profile", decode_enum)?;
        let mode = obj.required("mode", decode_enum)?;
        let object_format = obj.required("object_format", decode_enum)?;
        let repository_path = obj.field("repository");
        let repository = match de::nullable(obj.take("repository")?) {
            None => None,
            Some(value) => Some(decode_repository(&repository_path, value)?),
        };
        let forge_path = obj.field("forge");
        let forge = de::nullable(obj.take("forge")?)
            .map(|value| decode_enum(&forge_path, value))
            .transpose()?;
        let candidate_ref_path = obj.field("candidate_ref");
        let candidate_ref = match de::nullable(obj.take("candidate_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&candidate_ref_path, value)?),
        };
        let target_ref_path = obj.field("target_ref");
        let target_ref = match de::nullable(obj.take("target_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&target_ref_path, value)?),
        };
        let default_path = obj.field("default_branch_ref");
        let default_branch_ref = match de::nullable(obj.take("default_branch_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&default_path, value)?),
        };
        let base_path = obj.field("base_commit_oid");
        let base_commit = Oid::new(
            object_format,
            de::string(&base_path, obj.take("base_commit_oid")?)?,
        )
        .ok_or_else(|| Error::new(&base_path, ErrorKind::InvalidValue))?;
        let candidate_path = obj.field("candidate_commit_oid");
        let candidate_commit = match de::nullable(obj.take("candidate_commit_oid")?) {
            None => None,
            Some(value) => Some(
                Oid::new(object_format, de::string(&candidate_path, value)?)
                    .ok_or_else(|| Error::new(&candidate_path, ErrorKind::InvalidValue))?,
            ),
        };
        obj.finish()?;
        let consistent = match mode {
            RequestMode::CommitPair => candidate_commit.is_some(),
            RequestMode::Index => candidate_commit.is_none(),
        };
        if !consistent {
            return fail(&candidate_path, ErrorKind::Inconsistent);
        }
        let identity_fields = [
            repository.is_some(),
            candidate_ref.is_some(),
            target_ref.is_some(),
            default_branch_ref.is_some(),
        ];
        if !identity_fields.iter().all(|present| *present)
            && identity_fields.iter().any(|present| *present)
            || forge.is_some() && repository.is_none()
            || matches!(forge, Some(ForgeDialect::Github | ForgeDialect::Gitea))
                && repository
                    .as_ref()
                    .is_some_and(|identity| identity.owner().contains('/'))
        {
            return fail(&forge_path, ErrorKind::Inconsistent);
        }
        Ok(Self {
            profile,
            mode,
            object_format,
            repository,
            forge,
            candidate_ref,
            target_ref,
            default_branch_ref,
            base_commit,
            candidate_commit,
        })
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
        checked_canonical(&evaluation_value(self), Self::parse)
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
    let _canonical = evaluation.canonical_bytes().ok()?;
    let candidate_commit = match (evaluation.mode, evaluation.candidate_commit.as_ref()) {
        (RequestMode::CommitPair, Some(candidate)) => candidate,
        (RequestMode::CommitPair | RequestMode::Index, None | Some(_)) => return None,
    };
    let format = evaluation.object_format;
    Oid::new(format, base_tree.as_str().to_owned())?;
    Oid::new(format, candidate_tree.as_str().to_owned())?;
    let value = object(vec![
        ("schema", text(CANDIDATE_IDENTITY_DOMAIN)),
        ("mode", text("commit-pair")),
        ("event_kind", text("explicit-commit-pair")),
        ("finality", text("explicit-replay")),
        (
            "repository",
            evaluation
                .repository
                .as_ref()
                .map_or(Value::Null, repository),
        ),
        (
            "candidate_ref",
            optional_text(evaluation.candidate_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "target_ref",
            optional_text(evaluation.target_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "default_branch_ref",
            optional_text(
                evaluation
                    .default_branch_ref
                    .as_ref()
                    .map(BranchRef::as_str),
            ),
        ),
        (
            "base",
            commit_snapshot_value(format, &evaluation.base_commit, base_tree),
        ),
        (
            "candidate",
            commit_snapshot_value(format, candidate_commit, candidate_tree),
        ),
        ("materialization", text("git-objects")),
        ("skip_worktree_paths", Value::Integer(0)),
        ("index_only_materialized_paths", Value::Integer(0)),
        (
            "forge",
            evaluation
                .forge
                .map_or(Value::Null, |forge| text(forge.as_ref())),
        ),
    ]);
    Some(hj(CANDIDATE_IDENTITY_DOMAIN, &value))
}

fn evaluation_value(request: &EvaluationRequest) -> Value {
    object(vec![
        ("schema", text(EVALUATION_REQUEST_SCHEMA)),
        ("profile", text(request.profile.as_ref())),
        ("mode", text(request.mode.as_ref())),
        ("object_format", text(request.object_format.as_ref())),
        (
            "repository",
            request.repository.as_ref().map_or(Value::Null, repository),
        ),
        (
            "forge",
            request
                .forge
                .map_or(Value::Null, |forge| text(forge.as_ref())),
        ),
        (
            "candidate_ref",
            optional_text(request.candidate_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "target_ref",
            optional_text(request.target_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "default_branch_ref",
            optional_text(request.default_branch_ref.as_ref().map(BranchRef::as_str)),
        ),
        ("base_commit_oid", text(request.base_commit.as_str())),
        (
            "candidate_commit_oid",
            optional_text(request.candidate_commit.as_ref().map(Oid::as_str)),
        ),
    ])
}

fn commit_snapshot_value(object_format: ObjectFormat, commit: &Oid, tree: &Oid) -> Value {
    object(vec![
        ("kind", text("git-commit")),
        ("object_format", text(object_format.as_ref())),
        ("commit_oid", text(commit.as_str())),
        ("tree_oid", text(tree.as_str())),
    ])
}

fn optional_text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, text)
}

fn decode_ref(path: &str, value: Value) -> Result<BranchRef, Error> {
    BranchRef::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}
