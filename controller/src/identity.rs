use std::borrow::Cow;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use amiss_wire::model::{BranchRef, Digest, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

mod tests;

#[derive(Clone, Copy)]
enum ByteClass {
    Namespace,
    Opaque,
}

impl ByteClass {
    const fn admits(self, byte: u8) -> bool {
        match self {
            Self::Namespace => {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
            }
            Self::Opaque => {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'_' | b':' | b'/' | b'@' | b'+' | b'-')
            }
        }
    }
}

const fn bounded(raw: &[u8], maximum: usize, class: ByteClass) -> bool {
    if raw.is_empty() || raw.len() > maximum {
        return false;
    }
    let mut rest = raw;
    while let Some((&byte, tail)) = rest.split_first() {
        rest = tail;
        if !class.admits(byte) {
            return false;
        }
    }
    true
}

/// The registry key for one provider family, in a lowercase DNS-label
/// grammar so it can never collide by case or whitespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct ProviderNamespace(Cow<'static, str>);

impl ProviderNamespace {
    const INVALID: &'static str = "invalid provider namespace";

    /// # Panics
    /// When the literal is not a namespace; `provider_namespace!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(Self::valid(raw.as_bytes()), "{}", Self::INVALID);
        Self(Cow::Borrowed(raw))
    }

    const fn valid(raw: &[u8]) -> bool {
        match raw.split_first() {
            Some((&first, _)) => {
                (first.is_ascii_lowercase() || first.is_ascii_digit())
                    && bounded(raw, 64, ByteClass::Namespace)
            }
            None => false,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProviderNamespace {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if Self::valid(raw.as_bytes()) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! provider_namespace {
    ($raw:literal) => {
        const { $crate::ProviderNamespace::from_static($raw) }
    };
}

/// One provider-issued opaque identifier: bounded printable bytes the
/// controller stores and compares but never interprets. Which role a value
/// plays is said by the field that holds it, not by a wrapper type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct OpaqueId(Cow<'static, str>);

impl OpaqueId {
    const INVALID: &'static str = "invalid opaque identifier";

    /// # Panics
    /// When the literal is not an opaque identifier; `opaque_id!` makes that a build error.
    #[must_use]
    pub const fn from_static(raw: &'static str) -> Self {
        assert!(Self::valid(raw.as_bytes()), "{}", Self::INVALID);
        Self(Cow::Borrowed(raw))
    }

    const fn valid(raw: &[u8]) -> bool {
        bounded(raw, 256, ByteClass::Opaque)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OpaqueId {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if Self::valid(raw.as_bytes()) {
            Ok(Self(Cow::Owned(raw)))
        } else {
            Err(Self::INVALID)
        }
    }
}

#[macro_export]
macro_rules! opaque_id {
    ($raw:literal) => {
        const { $crate::OpaqueId::from_static($raw) }
    };
}

/// A provider run attempt: one-based and inside the exact-integer range
/// every JSON consumer can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64")]
pub struct ProviderRunAttempt(u64);

impl ProviderRunAttempt {
    pub const FIRST: Self = Self(1);
    const INVALID: &'static str = "invalid provider run attempt";

    /// # Panics
    /// When `raw` is zero or beyond the exact-integer range; meant for literals.
    #[must_use]
    pub const fn literal(raw: u64) -> Self {
        assert!(Self::valid(raw), "{}", Self::INVALID);
        Self(raw)
    }

    const fn valid(raw: u64) -> bool {
        raw != 0 && raw <= 9_007_199_254_740_991
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for ProviderRunAttempt {
    type Error = &'static str;

    fn try_from(raw: u64) -> Result<Self, Self::Error> {
        if Self::valid(raw) {
            Ok(Self(raw))
        } else {
            Err(Self::INVALID)
        }
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
    pub instance: OpaqueId,
}

impl ProviderIdentity {
    pub fn new(namespace: String, instance: String) -> Option<Self> {
        Some(Self {
            namespace: ProviderNamespace::try_from(namespace).ok()?,
            instance: OpaqueId::try_from(instance).ok()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryIdentity {
    pub provider: ProviderIdentity,
    pub integration: OpaqueId,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    Active,
    Superseded,
    Closed,
    AuthorizationRevoked,
}

/// The refs one run resolves against.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRefs {
    pub forge: ForgeDialect,
    pub candidate: BranchRef,
    pub target: BranchRef,
    pub default_branch: BranchRef,
}

/// One base and candidate pair of object ids.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidPair {
    pub base: Oid,
    pub candidate: Oid,
}

/// The exact identity one evaluation runs as. Everything here is data; the
/// binding laws live in `validate_change` and the runner recheck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIdentity {
    pub change: ChangeLocator,
    pub refs: RunRefs,
    pub object_format: ObjectFormat,
    pub commits: OidPair,
    pub trees: OidPair,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSnapshot {
    pub state: ChangeState,
    pub run: RunIdentity,
    /// Provider revision to which the adapter binds this run's gate.
    pub gate_commit: Oid,
}

/// What a provider authenticated about one delivery. Which delivery it is
/// stays the ingress's to say, from the replay identity the proof carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderFacts {
    pub provider: ProviderIdentity,
    pub integration: OpaqueId,
    pub change: ChangeLocator,
    pub provider_run: ProviderRunIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDelivery {
    pub identity: DeliveryIdentity,
    pub change: ChangeLocator,
    pub provider_run: ProviderRunIdentity,
}
