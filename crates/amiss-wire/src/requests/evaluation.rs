use serde::{Deserialize, Serialize};

use crate::controls::{Profile, root};
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::RequestMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvaluationRequestSchema {
    #[serde(rename = "amiss/scanner-evaluation-request")]
    Current,
}

/// The run-identity request: profile, mode, and the exact snapshot
/// identities to evaluate. The candidate commit is null exactly when the
/// mode is `index`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationRequest {
    pub schema: EvaluationRequestSchema,
    pub profile: Profile,
    pub mode: RequestMode,
    pub object_format: ObjectFormat,
    #[serde(deserialize_with = "Option::deserialize")]
    pub repository: Option<RepositoryIdentity>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub forge: Option<ForgeDialect>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub candidate_ref: Option<BranchRef>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target_ref: Option<BranchRef>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub default_branch_ref: Option<BranchRef>,
    #[serde(rename = "base_commit_oid")]
    pub base_commit: Oid,
    #[serde(
        rename = "candidate_commit_oid",
        deserialize_with = "Option::deserialize"
    )]
    pub candidate_commit: Option<Oid>,
}

impl EvaluationRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid
    /// grammar values, and a candidate commit inconsistent with the mode.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        root(bytes)?;
        let request: Self = de::deserialize_json(bytes)?;
        validate_evaluation(&request)?;
        Ok(request)
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
            schema: EvaluationRequestSchema::Current,
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
        validate_evaluation(self)?;
        serde_json_canonicalizer::to_vec(self)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))
    }
}

fn validate_evaluation(request: &EvaluationRequest) -> Result<(), Error> {
    if request.repository.as_ref().is_some_and(|repository| {
        RepositoryIdentity::new(
            repository.host().to_owned(),
            repository.owner().to_owned(),
            repository.name().to_owned(),
        )
        .as_ref()
            != Some(repository)
    }) {
        return fail("$.repository", ErrorKind::InvalidValue);
    }
    for (path, oid) in [
        ("$.base_commit_oid", Some(&request.base_commit)),
        ("$.candidate_commit_oid", request.candidate_commit.as_ref()),
    ] {
        if oid.is_some_and(|value| value.object_format() != request.object_format) {
            return fail(path, ErrorKind::InvalidValue);
        }
    }
    ((request.mode == RequestMode::CommitPair) == request.candidate_commit.is_some())
        .then_some(())
        .ok_or_else(|| Error::new("$.candidate_commit_oid", ErrorKind::Inconsistent))?;

    let repository_present = request.repository.is_some();
    let identity_is_complete = [
        request.candidate_ref.is_some(),
        request.target_ref.is_some(),
        request.default_branch_ref.is_some(),
    ]
    .into_iter()
    .all(|present| present == repository_present);
    let forge_is_coherent = repository_present || request.forge.is_none();
    let owner_is_coherent = request.forge.is_none()
        || request.forge == Some(ForgeDialect::Gitlab)
        || request
            .repository
            .as_ref()
            .is_none_or(|repository| !repository.owner().contains('/'));
    (identity_is_complete && forge_is_coherent && owner_is_coherent)
        .then_some(())
        .ok_or_else(|| Error::new("$.forge", ErrorKind::Inconsistent))
}
