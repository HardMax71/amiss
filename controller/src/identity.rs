use std::fmt;

use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

fn bounded(raw: String, maximum: usize, valid: impl Fn(u8) -> bool) -> Option<String> {
    let bytes = raw.as_bytes();
    (!bytes.is_empty() && bytes.len() <= maximum && bytes.iter().all(|byte| valid(*byte)))
        .then_some(raw)
}

/// The registry key for one provider family, in a lowercase DNS-label
/// grammar so it can never collide by case or whitespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpaqueId(String);

pub type ProviderInstance = OpaqueId;
pub type IntegrationId = OpaqueId;
pub type DeliveryId = OpaqueId;
pub type ChangeId = OpaqueId;
pub type ProviderRunId = OpaqueId;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

/// A provider run pinned to the delivery-authenticated candidate commit
/// before any refresh can substitute a newer head.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderRunIdentity {
    pub run_id: ProviderRunId,
    pub attempt: ProviderRunAttempt,
    pub object_format: ObjectFormat,
    pub candidate_commit: Oid,
}

impl ProviderRunIdentity {
    /// None unless the candidate commit is well formed for the object format.
    pub fn new(
        run_id: ProviderRunId,
        attempt: ProviderRunAttempt,
        object_format: ObjectFormat,
        candidate_commit: Oid,
    ) -> Option<Self> {
        Oid::new(object_format, candidate_commit.as_str().to_owned())?;
        Some(Self {
            run_id,
            attempt,
            object_format,
            candidate_commit,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderIdentity {
    pub namespace: ProviderNamespace,
    pub instance: ProviderInstance,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeliveryIdentity {
    pub provider: ProviderIdentity,
    pub integration: IntegrationId,
    pub delivery: DeliveryId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangeLocator {
    pub provider: ProviderIdentity,
    pub repository: RepositoryIdentity,
    pub change: ChangeId,
}
