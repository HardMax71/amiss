use super::mapping::wire_fields;
use super::{check_schema, root};
use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ArtifactId, BranchRef, RepositoryIdentity, UtcInstant};
use serde::{Deserialize, Serialize};

const TRUSTED_TIME_STATEMENT_SCHEMA: &str = "amiss/scanner-trusted-time-statement";
const TRUSTED_TIME_CONTROLLER: &str = "external-required-check-clock";

/// The controller's maximum statement lifetime: `evaluation_instant <
/// valid_until <= evaluation_instant + 600` whole seconds.
pub const STATEMENT_TTL_MAX_SECONDS: i64 = 600;

/// A trusted-time statement issued by the required-check clock inside the
/// externally controlled run. Parsing establishes shape and the TTL law; the
/// evaluation-side bindings (repository, ref, candidate identity, run,
/// attempt) are separate verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedTimeStatement {
    digest: Digest,
    input: TrustedTimeInput,
}

/// The controller-owned fields of a trusted-time statement. The schema,
/// controller identity, and digest are fixed or derived by the wire type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedTimeInput {
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub candidate_identity_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

impl TrustedTimeStatement {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn repository(&self) -> &RepositoryIdentity {
        &self.input.repository
    }

    #[must_use]
    pub fn ref_name(&self) -> &BranchRef {
        &self.input.ref_name
    }

    #[must_use]
    pub const fn candidate_identity_digest(&self) -> Digest {
        self.input.candidate_identity_digest
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.input.provider
    }

    #[must_use]
    pub fn provider_run_id(&self) -> &str {
        &self.input.provider_run_id
    }

    #[must_use]
    pub const fn provider_run_attempt(&self) -> u64 {
        self.input.provider_run_attempt
    }

    #[must_use]
    pub fn evaluation_instant(&self) -> &UtcInstant {
        &self.input.evaluation_instant
    }

    #[must_use]
    pub fn valid_until(&self) -> &UtcInstant {
        &self.input.valid_until
    }

    /// Builds a statement through the same grammar, lifetime, and digest
    /// rules used for untrusted wire bytes. The lifetime check is internal to
    /// the statement: the issuer must still source `evaluation_instant` from
    /// controller-owned current time and bind the statement to the exact run.
    ///
    /// # Errors
    ///
    /// A field violates [`Self::parse`], including a non-positive or
    /// unrepresentable run attempt or a lifetime outside the allowed window.
    pub fn new(input: TrustedTimeInput) -> Result<Self, Error> {
        let payload = Statement::from(input);
        payload.check()?;
        let digest = codec::digest(TRUSTED_TIME_STATEMENT_SCHEMA, &payload)?;
        Ok(Self::from_payload(payload, digest))
    }

    #[must_use]
    pub const fn schema(&self) -> &'static str {
        TRUSTED_TIME_STATEMENT_SCHEMA
    }

    #[must_use]
    pub const fn controller(&self) -> &'static str {
        TRUSTED_TIME_CONTROLLER
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid grammar
    /// values, and a lifetime outside `0 < valid_until - evaluation_instant
    /// <= 600` seconds.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_value(&root(bytes)?)
    }

    /// Checks an already decoded control without serializing it again.
    ///
    /// # Errors
    ///
    /// A field violates the closed shape or the control's domain laws.
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let payload: Statement = codec::from_value("$", value)?;
        payload.check()?;
        Ok(Self::from_payload(
            payload,
            hj(TRUSTED_TIME_STATEMENT_SCHEMA, value),
        ))
    }

    fn from_payload(payload: Statement, digest: Digest) -> Self {
        Self {
            digest,
            input: payload.into(),
        }
    }

    /// Serializes one valid statement to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The stored statement violates its domain laws or its derived digest
    /// does not match the canonical value.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        let payload = Statement::from(TrustedTimeInput::from(self));
        payload.check()?;
        if codec::digest(TRUSTED_TIME_STATEMENT_SCHEMA, &payload)? != self.digest {
            return fail("$.digest", ErrorKind::DigestMismatch);
        }
        codec::canonical(&payload)
    }
}

impl From<&TrustedTimeStatement> for TrustedTimeInput {
    fn from(value: &TrustedTimeStatement) -> Self {
        value.input.clone()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    schema: String,
    controller: String,
    repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    ref_name: BranchRef,
    candidate_identity_digest: Digest,
    provider: String,
    provider_run_id: String,
    provider_run_attempt: u64,
    evaluation_instant: UtcInstant,
    valid_until: UtcInstant,
}

wire_fields! {
    Statement <=> TrustedTimeInput (input) {
        fields [
            repository,
            ref_name,
            candidate_identity_digest,
            provider,
            provider_run_id,
            provider_run_attempt,
            evaluation_instant,
            valid_until,
        ],
        mapped [],
        wire {
            schema: TRUSTED_TIME_STATEMENT_SCHEMA.to_owned(),
            controller: TRUSTED_TIME_CONTROLLER.to_owned(),
        },
        domain {}
    }
}

impl Statement {
    fn check(&self) -> Result<(), Error> {
        check_schema("$.schema", &self.schema, TRUSTED_TIME_STATEMENT_SCHEMA)?;
        check_schema("$.controller", &self.controller, TRUSTED_TIME_CONTROLLER)?;
        if ArtifactId::new(self.provider.clone()).is_none() {
            return fail("$.provider", ErrorKind::InvalidValue);
        }
        if !super::valid_provider_run_id(&self.provider_run_id) {
            return fail("$.provider_run_id", ErrorKind::InvalidValue);
        }
        if !(1..=codec::MAX_SAFE_INTEGER).contains(&self.provider_run_attempt) {
            return fail("$.provider_run_attempt", ErrorKind::InvalidValue);
        }
        let lifetime = self
            .valid_until
            .epoch_seconds()
            .saturating_sub(self.evaluation_instant.epoch_seconds());
        if !(1..=STATEMENT_TTL_MAX_SECONDS).contains(&lifetime) {
            return fail("$.valid_until", ErrorKind::InvalidValue);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct StatementView<'a> {
    candidate_identity_digest: Digest,
    controller: &'static str,
    evaluation_instant: &'a UtcInstant,
    provider: &'a str,
    provider_run_attempt: u64,
    provider_run_id: &'a str,
    #[serde(rename = "ref")]
    ref_name: &'a BranchRef,
    repository: &'a RepositoryIdentity,
    schema: &'static str,
    valid_until: &'a UtcInstant,
}

impl Serialize for TrustedTimeStatement {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StatementView {
            candidate_identity_digest: self.input.candidate_identity_digest,
            controller: TRUSTED_TIME_CONTROLLER,
            evaluation_instant: &self.input.evaluation_instant,
            provider: &self.input.provider,
            provider_run_attempt: self.input.provider_run_attempt,
            provider_run_id: &self.input.provider_run_id,
            ref_name: &self.input.ref_name,
            repository: &self.input.repository,
            schema: TRUSTED_TIME_STATEMENT_SCHEMA,
            valid_until: &self.input.valid_until,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TrustedTimeStatement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let payload = Statement::deserialize(codec::object(deserializer))?;
        payload.check().map_err(serde::de::Error::custom)?;
        let digest = codec::digest(TRUSTED_TIME_STATEMENT_SCHEMA, &payload)
            .map_err(serde::de::Error::custom)?;
        Ok(Self::from_payload(payload, digest))
    }
}
