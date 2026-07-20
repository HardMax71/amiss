use std::fmt;

use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderNamespace(String);

impl ProviderNamespace {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        let mut bytes = raw.bytes();
        let first = bytes.next()?;
        if raw.len() > 64 || !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return None;
        }
        if bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        }) {
            Some(Self(raw))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn opaque_id_valid(raw: &str, maximum: usize) -> bool {
    let bytes = raw.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= maximum
        && bytes.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'.' | b'_' | b':' | b'/' | b'@' | b'+' | b'-')
        })
}

macro_rules! opaque_id {
    ($name:ident, $maximum:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(raw: String) -> Option<Self> {
                if opaque_id_valid(&raw, $maximum) {
                    Some(Self(raw))
                } else {
                    None
                }
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

opaque_id!(ProviderInstance, 255);
opaque_id!(IntegrationId, 128);
opaque_id!(DeliveryId, 256);
opaque_id!(ChangeId, 256);
opaque_id!(ProviderRunId, 128);
opaque_id!(ControllerEvaluationId, 128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderRunAttempt(u64);

impl ProviderRunAttempt {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 || raw > 9_007_199_254_740_991 {
            None
        } else {
            Some(Self(raw))
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderRunIdentity {
    run_id: ProviderRunId,
    attempt: ProviderRunAttempt,
    object_format: ObjectFormat,
    candidate_commit: Oid,
}

impl ProviderRunIdentity {
    /// Binds the provider run and attempt to the candidate commit authenticated
    /// from the delivery before an authoritative refresh is attempted.
    #[must_use]
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

    #[must_use]
    pub const fn run_id(&self) -> &ProviderRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn attempt(&self) -> ProviderRunAttempt {
        self.attempt
    }

    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    #[must_use]
    pub const fn candidate_commit(&self) -> &Oid {
        &self.candidate_commit
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
