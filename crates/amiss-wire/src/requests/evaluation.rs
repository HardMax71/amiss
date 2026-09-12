use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::Profile;
use crate::de::{self, Error, ErrorKind, fail};
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::RequestMode;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvaluationRequestSchema {
    #[strum(serialize = "amiss/scanner-evaluation-request")]
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
    /// Fails on JSON defects, schema-shape violations, invalid
    /// grammar values, and a candidate commit inconsistent with the mode.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let request: Self = serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|defect| de::deserialize_error("$", &defect))?;
        deserializer
            .end()
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;

        request.validate()?;
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

    /// Checks the run's object format, mode, and complete forge identity.
    ///
    /// # Errors
    /// Refuses inconsistent commits, repository identity, or forge bindings.
    pub fn validate(&self) -> Result<(), Error> {
        if self.repository.as_ref().is_some_and(|repository| {
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
            ("$.base_commit_oid", Some(&self.base_commit)),
            ("$.candidate_commit_oid", self.candidate_commit.as_ref()),
        ] {
            if oid.is_some_and(|value| value.object_format() != self.object_format) {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        ((self.mode == RequestMode::CommitPair) == self.candidate_commit.is_some())
            .then_some(())
            .ok_or_else(|| Error::new("$.candidate_commit_oid", ErrorKind::Inconsistent))?;

        let repository_present = self.repository.is_some();
        let identity_is_complete = [
            self.candidate_ref.is_some(),
            self.target_ref.is_some(),
            self.default_branch_ref.is_some(),
        ]
        .into_iter()
        .all(|present| present == repository_present);
        let forge_is_coherent = repository_present || self.forge.is_none();
        let owner_is_coherent = self.forge.is_none()
            || self.forge == Some(ForgeDialect::Gitlab)
            || self
                .repository
                .as_ref()
                .is_none_or(|repository| !repository.owner().contains('/'));
        (identity_is_complete && forge_is_coherent && owner_is_coherent)
            .then_some(())
            .ok_or_else(|| Error::new("$.forge", ErrorKind::Inconsistent))
    }
}
