use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind};
use crate::digest::{Digest, hb};
use crate::json::MAX_SAFE_INTEGER;
use crate::model::{ArtifactId, BranchRef, RepositoryIdentity, UtcInstant};

use super::{provider_run_id_valid, validate_repository};

pub const TRUSTED_TIME_STATEMENT_SCHEMA: &str = "amiss/scanner-trusted-time-statement";
pub const TRUSTED_TIME_CONTROLLER: &str = "external-required-check-clock";

/// The controller's maximum statement lifetime: `evaluation_instant <
/// valid_until <= evaluation_instant + 600` whole seconds.
pub const STATEMENT_TTL_MAX_SECONDS: i64 = 600;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum TrustedTimeSchema {
    #[strum(serialize = "amiss/scanner-trusted-time-statement")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum TrustedTimeController {
    #[strum(serialize = "external-required-check-clock")]
    ExternalRequiredCheckClock,
}

/// A trusted-time statement issued by the required-check clock inside the
/// externally controlled run. Its evaluation-side bindings remain separate
/// verification.
#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedTimeStatement {
    pub candidate_identity_digest: Digest,
    pub controller: TrustedTimeController,
    pub evaluation_instant: UtcInstant,
    pub provider: String,
    pub provider_run_attempt: u64,
    pub provider_run_id: String,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub repository: RepositoryIdentity,
    pub schema: TrustedTimeSchema,
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
    let (statement, _digest) = de::deserialize_json(bytes, TRUSTED_TIME_STATEMENT_SCHEMA)?;
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
    validate_repository("$.repository", &statement.repository)?;
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
    let [evaluation, valid_until] =
        [&statement.evaluation_instant, &statement.valid_until].map(|instant| {
            let edtf_core::Edtf::DateTime(instant) = edtf_core::Edtf::parse(instant).ok()? else {
                return None;
            };
            Some(datealgo::datetime_to_secs((
                instant.date.year.value()?.try_into().ok()?,
                instant.date.month?.value()?,
                instant.date.day?.value()?,
                instant.time.hour,
                instant.time.minute,
                instant.time.second,
            )))
        });
    evaluation
        .zip(valid_until)
        .map(|(evaluation, until)| until.saturating_sub(evaluation))
        .filter(|lifetime| (1..=STATEMENT_TTL_MAX_SECONDS).contains(lifetime))
        .map(|_lifetime| ())
        .ok_or_else(|| Error::new("$.valid_until", ErrorKind::InvalidValue))
}
