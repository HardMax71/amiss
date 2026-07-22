use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{Value, canonical};
use crate::model::{BranchRef, RepositoryIdentity, UtcInstant};

use super::value::{object, positive_safe_integer, repository, text};
use super::{
    decode_branch_ref, decode_digest, decode_instant, decode_provider_id, decode_provider_run_id,
    decode_repository, root,
};

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
    pub digest: Digest,
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub candidate_identity_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
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
        Self::parse(&canonical(&trusted_time_value(input)?))
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
        let value = root(bytes)?;
        let digest = hj(TRUSTED_TIME_STATEMENT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            TRUSTED_TIME_STATEMENT_SCHEMA,
        )?;
        de::const_str(
            &obj.field("controller"),
            obj.take("controller")?,
            TRUSTED_TIME_CONTROLLER,
        )?;
        let repository = decode_repository(&obj.field("repository"), obj.take("repository")?)?;
        let ref_name = decode_branch_ref(&obj.field("ref"), obj.take("ref")?)?;
        let candidate_identity_digest = decode_digest(
            &obj.field("candidate_identity_digest"),
            obj.take("candidate_identity_digest")?,
        )?;
        let provider = decode_provider_id(&obj.field("provider"), obj.take("provider")?)?;
        let run_id_path = obj.field("provider_run_id");
        let provider_run_id = decode_provider_run_id(&run_id_path, obj.take("provider_run_id")?)?;
        let attempt_path = obj.field("provider_run_attempt");
        let attempt_raw = de::integer(&attempt_path, obj.take("provider_run_attempt")?)?;
        let provider_run_attempt = u64::try_from(attempt_raw)
            .ok()
            .filter(|attempt| *attempt >= 1)
            .ok_or_else(|| Error::new(&attempt_path, ErrorKind::InvalidValue))?;
        let evaluation_instant = decode_instant(
            &obj.field("evaluation_instant"),
            obj.take("evaluation_instant")?,
        )?;
        let until_path = obj.field("valid_until");
        let valid_until = decode_instant(&until_path, obj.take("valid_until")?)?;
        obj.finish()?;
        let lifetime = valid_until
            .epoch_seconds()
            .saturating_sub(evaluation_instant.epoch_seconds());
        if lifetime <= 0 || lifetime > STATEMENT_TTL_MAX_SECONDS {
            return fail(&until_path, ErrorKind::InvalidValue);
        }
        Ok(Self {
            digest,
            repository,
            ref_name,
            candidate_identity_digest,
            provider,
            provider_run_id,
            provider_run_attempt,
            evaluation_instant,
            valid_until,
        })
    }

    /// Serializes one valid statement to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// A public field was changed into a value [`Self::parse`] rejects, or
    /// changed without replacing the derived `digest`.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        let bytes = canonical(&trusted_time_value(self.into())?);
        let parsed = Self::parse(&bytes)?;
        if parsed.digest != self.digest {
            return fail("$.digest", ErrorKind::DigestMismatch);
        }
        Ok(bytes)
    }
}

impl From<&TrustedTimeStatement> for TrustedTimeInput {
    fn from(statement: &TrustedTimeStatement) -> Self {
        Self {
            repository: statement.repository.clone(),
            ref_name: statement.ref_name.clone(),
            candidate_identity_digest: statement.candidate_identity_digest,
            provider: statement.provider.clone(),
            provider_run_id: statement.provider_run_id.clone(),
            provider_run_attempt: statement.provider_run_attempt,
            evaluation_instant: statement.evaluation_instant.clone(),
            valid_until: statement.valid_until.clone(),
        }
    }
}

fn trusted_time_value(input: TrustedTimeInput) -> Result<Value, Error> {
    let provider_run_attempt =
        positive_safe_integer("$.provider_run_attempt", input.provider_run_attempt)?;
    let TrustedTimeInput {
        repository: repository_identity,
        ref_name,
        candidate_identity_digest,
        provider,
        provider_run_id,
        provider_run_attempt: _,
        evaluation_instant,
        valid_until,
    } = input;
    Ok(object(vec![
        ("schema", text(TRUSTED_TIME_STATEMENT_SCHEMA)),
        ("controller", text(TRUSTED_TIME_CONTROLLER)),
        ("repository", repository(&repository_identity)),
        ("ref", text(ref_name.as_str())),
        (
            "candidate_identity_digest",
            text(&candidate_identity_digest.to_string()),
        ),
        ("provider", Value::String(provider)),
        ("provider_run_id", Value::String(provider_run_id)),
        ("provider_run_attempt", provider_run_attempt),
        ("evaluation_instant", text(evaluation_instant.as_str())),
        ("valid_until", text(valid_until.as_str())),
    ]))
}
