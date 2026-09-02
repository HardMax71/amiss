use serde::{Deserialize, Serialize};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json::MAX_SAFE_INTEGER;
use crate::model::{ArtifactId, BranchRef, RepositoryIdentity, UtcInstant};

use super::{provider_run_id_valid, root};

pub const TRUSTED_TIME_STATEMENT_SCHEMA: &str = "amiss/scanner-trusted-time-statement";
pub const TRUSTED_TIME_CONTROLLER: &str = "external-required-check-clock";

/// The controller's maximum statement lifetime: `evaluation_instant <
/// valid_until <= evaluation_instant + 600` whole seconds.
pub const STATEMENT_TTL_MAX_SECONDS: i64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustedTimeSchema {
    #[serde(rename = "amiss/scanner-trusted-time-statement")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustedTimeController {
    #[serde(rename = "external-required-check-clock")]
    ExternalRequiredCheckClock,
}

/// A trusted-time statement issued by the required-check clock inside the
/// externally controlled run. Its evaluation-side bindings remain separate
/// verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedTimeStatement {
    pub schema: TrustedTimeSchema,
    pub controller: TrustedTimeController,
    pub repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    pub candidate_identity_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

/// Parses and validates one trusted-time statement.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, invalid grammar
/// values, or a lifetime outside `0 < valid_until - evaluation_instant <= 600`
/// seconds.
pub fn parse_trusted_time(bytes: &[u8]) -> Result<TrustedTimeStatement, Error> {
    root(bytes)?;
    let statement = de::deserialize_json(bytes)?;
    validate_trusted_time(&statement)?;
    Ok(statement)
}

/// Produces one valid statement's canonical bytes and their domain-separated
/// digest together.
///
/// # Errors
///
/// A public field violates the same laws [`parse_trusted_time`] enforces, or
/// the typed value cannot be serialized.
pub fn canonical_trusted_time(
    statement: &TrustedTimeStatement,
) -> Result<(Vec<u8>, Digest), Error> {
    validate_trusted_time(statement)?;
    let bytes = serde_json_canonicalizer::to_vec(statement)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(TRUSTED_TIME_STATEMENT_SCHEMA, &bytes);
    Ok((bytes, digest))
}

fn validate_trusted_time(statement: &TrustedTimeStatement) -> Result<(), Error> {
    if RepositoryIdentity::new(
        statement.repository.host().to_owned(),
        statement.repository.owner().to_owned(),
        statement.repository.name().to_owned(),
    )
    .as_ref()
        != Some(&statement.repository)
    {
        return fail("$.repository", ErrorKind::InvalidValue);
    }
    ArtifactId::new(statement.provider.clone())
        .is_some()
        .then_some(())
        .ok_or_else(|| Error::new("$.provider", ErrorKind::InvalidValue))?;
    provider_run_id_valid(&statement.provider_run_id)
        .then_some(())
        .ok_or_else(|| Error::new("$.provider_run_id", ErrorKind::InvalidValue))?;
    (1..=MAX_SAFE_INTEGER.unsigned_abs())
        .contains(&statement.provider_run_attempt)
        .then_some(())
        .ok_or_else(|| Error::new("$.provider_run_attempt", ErrorKind::InvalidValue))?;
    for (path, instant) in [
        ("$.evaluation_instant", &statement.evaluation_instant),
        ("$.valid_until", &statement.valid_until),
    ] {
        if UtcInstant::new(instant.as_str().to_owned()).as_ref() != Some(instant) {
            return fail(path, ErrorKind::InvalidValue);
        }
    }
    let lifetime = statement
        .valid_until
        .epoch_seconds()
        .saturating_sub(statement.evaluation_instant.epoch_seconds());
    (1..=STATEMENT_TTL_MAX_SECONDS)
        .contains(&lifetime)
        .then_some(())
        .ok_or_else(|| Error::new("$.valid_until", ErrorKind::InvalidValue))
}
