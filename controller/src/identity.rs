use std::num::NonZeroU64;
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

use amiss_wire::model::{Digest, ObjectFormat, Oid, RepositoryIdentity};

mod tests;

fn bounded(raw: String, maximum: usize, valid: impl Fn(u8) -> bool) -> Option<String> {
    let bytes = raw.as_bytes();
    (!bytes.is_empty() && bytes.len() <= maximum && bytes.iter().all(|byte| valid(*byte)))
        .then_some(raw)
}

/// The registry key for one provider family, in a lowercase DNS-label
/// grammar so it can never collide by case or whitespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializeDisplay, DeserializeFromStr)]
pub struct ProviderNamespace(String);

impl ProviderNamespace {
    pub fn new(raw: String) -> Option<Self> {
        let first = *raw.as_bytes().first()?;
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return None;
        }
        bounded(raw, 64, |byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        .map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One provider-issued opaque identifier: bounded printable bytes the
/// controller stores and compares but never interprets. Which role a value
/// plays is said by the field that holds it, not by a wrapper type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SerializeDisplay, DeserializeFromStr)]
pub struct OpaqueId(String);

pub type ProviderInstance = OpaqueId;
pub type IntegrationId = OpaqueId;
pub type ControllerEvaluationId = OpaqueId;

impl OpaqueId {
    pub fn new(raw: String) -> Option<Self> {
        bounded(raw, 256, |byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b':' | b'/' | b'@' | b'+' | b'-')
        })
        .map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpaqueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A provider run attempt: one-based and inside the exact-integer range
/// every JSON consumer can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct ProviderRunAttempt(u64);

impl ProviderRunAttempt {
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 || raw > 9_007_199_254_740_991 {
            None
        } else {
            Some(Self(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for ProviderRunAttempt {
    type Error = &'static str;

    fn try_from(raw: u64) -> Result<Self, Self::Error> {
        Self::new(raw).ok_or("invalid provider run attempt")
    }
}

impl From<ProviderRunAttempt> for u64 {
    fn from(attempt: ProviderRunAttempt) -> Self {
        attempt.get()
    }
}

/// A provider run pinned to the delivery-authenticated candidate commit
/// before any refresh can substitute a newer head.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRunIdentity {
    pub run: ProviderRun,
    pub attempt: ProviderRunAttempt,
    pub object_format: ObjectFormat,
    pub candidate_commit: Oid,
}

impl ProviderRunIdentity {
    /// None unless the candidate commit is well formed for the object format.
    pub fn new(
        run: ProviderRun,
        attempt: ProviderRunAttempt,
        object_format: ObjectFormat,
        candidate_commit: Oid,
    ) -> Option<Self> {
        let identity = Self {
            run,
            attempt,
            object_format,
            candidate_commit,
        };
        identity.is_valid().then_some(identity)
    }

    pub fn is_valid(&self) -> bool {
        self.candidate_commit.object_format() == self.object_format
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderIdentity {
    pub namespace: ProviderNamespace,
    pub instance: ProviderInstance,
}

impl ProviderIdentity {
    pub fn new(namespace: String, instance: String) -> Option<Self> {
        Some(Self {
            namespace: ProviderNamespace::new(namespace)?,
            instance: ProviderInstance::new(instance)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryIdentity {
    pub provider: ProviderIdentity,
    pub integration: IntegrationId,
    pub delivery: Delivery,
}

/// What one delivery is, once and only once: the id the provider put on it,
/// the exact body when the provider signs nothing else, or the token a GitLab
/// runner presented.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Delivery {
    Provided(OpaqueId),
    Body(Digest),
    Token(OidcToken),
}

/// One GitLab OIDC token: the runner that presented it and the digest of its
/// token id, which the provider never reuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcToken {
    pub runner_id: NonZeroU64,
    pub jti: Digest,
}

impl OidcToken {
    /// None when the provider issued a zero runner id.
    #[must_use]
    pub fn new(runner_id: u64, jti: Digest) -> Option<Self> {
        Some(Self {
            runner_id: NonZeroU64::new(runner_id)?,
            jti,
        })
    }
}

/// Which provider run a delivery came from. The GitHub family has no run of
/// its own for a pull request event, so the run is the digest of the fields
/// the delivery bound; GitLab names the job inside its pipeline.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderRun {
    PullRequest(Digest),
    Job(PipelineJob),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineJob {
    pub pipeline_id: NonZeroU64,
    pub job_id: NonZeroU64,
}

impl PipelineJob {
    /// None when the provider issued a zero for either.
    #[must_use]
    pub fn new(pipeline_id: u64, job_id: u64) -> Option<Self> {
        Some(Self {
            pipeline_id: NonZeroU64::new(pipeline_id)?,
            job_id: NonZeroU64::new(job_id)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangeLocator {
    pub provider: ProviderIdentity,
    pub repository: RepositoryIdentity,
    pub change: Change,
}

/// Which change a delivery is about, numbered the way its provider family
/// numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    PullRequest(PullRequestChange),
    MergeRequest(MergeRequestChange),
}

/// A GitHub-family pull request: the repository and the pull request by id,
/// and the pull request by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestChange {
    pub repository_id: NonZeroU64,
    pub pull_request_id: NonZeroU64,
    pub number: NonZeroU64,
}

impl PullRequestChange {
    /// None when the provider issued a zero for any of the three.
    #[must_use]
    pub fn new(repository_id: u64, pull_request_id: u64, number: u64) -> Option<Self> {
        Some(Self {
            repository_id: NonZeroU64::new(repository_id)?,
            pull_request_id: NonZeroU64::new(pull_request_id)?,
            number: NonZeroU64::new(number)?,
        })
    }
}

/// A GitLab merge request: the project by id and the merge request by the
/// iid the project counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeRequestChange {
    pub project_id: NonZeroU64,
    pub iid: NonZeroU64,
}

impl MergeRequestChange {
    /// None when the provider issued a zero for either.
    #[must_use]
    pub fn new(project_id: u64, iid: u64) -> Option<Self> {
        Some(Self {
            project_id: NonZeroU64::new(project_id)?,
            iid: NonZeroU64::new(iid)?,
        })
    }
}

impl FromStr for ProviderNamespace {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::new(raw.to_owned()).ok_or("invalid provider namespace")
    }
}

impl FromStr for OpaqueId {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::new(raw.to_owned()).ok_or("invalid opaque identifier")
    }
}
