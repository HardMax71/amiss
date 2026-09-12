use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind};
use crate::model::Digest;
use crate::model::{ArtifactId, BranchRef, RepositoryIdentity, UtcInstant};
use js_int::MAX_SAFE_INT as MAX_SAFE_INTEGER;

use super::{provider_run_id_valid, validate_instant, validate_repository};

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
    pub repository: RepositoryIdentity,
    pub schema: TrustedTimeSchema,
    pub valid_until: UtcInstant,
}

/// Parses and validates one trusted-time statement.
///
/// # Errors
///
/// Fails on JSON defects, schema-shape violations, invalid grammar
/// values, or a lifetime outside `0 < valid_until - evaluation_instant <= 600`
/// seconds.
pub fn parse_trusted_time(bytes: &[u8]) -> Result<TrustedTimeStatement, Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let statement: TrustedTimeStatement = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;

    statement.validate()?;
    Ok(statement)
}

impl TrustedTimeStatement {
    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_trusted_time`].
    pub fn validate(&self) -> Result<(), Error> {
        validate_repository("$.repository", &self.repository)?;
        ArtifactId::new(self.provider.clone())
            .is_some()
            .then_some(())
            .ok_or_else(|| Error::new("$.provider", ErrorKind::InvalidValue))?;
        provider_run_id_valid(&self.provider_run_id)
            .then_some(())
            .ok_or_else(|| Error::new("$.provider_run_id", ErrorKind::InvalidValue))?;
        (1..=MAX_SAFE_INTEGER.unsigned_abs())
            .contains(&self.provider_run_attempt)
            .then_some(())
            .ok_or_else(|| Error::new("$.provider_run_attempt", ErrorKind::InvalidValue))?;
        validate_instant("$.evaluation_instant", &self.evaluation_instant)?;
        validate_instant("$.valid_until", &self.valid_until)?;
        let lifetime = self
            .valid_until
            .epoch_seconds()
            .saturating_sub(self.evaluation_instant.epoch_seconds());
        (1..=STATEMENT_TTL_MAX_SECONDS)
            .contains(&lifetime)
            .then_some(())
            .ok_or_else(|| Error::new("$.valid_until", ErrorKind::InvalidValue))
    }
}
