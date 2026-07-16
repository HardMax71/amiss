use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::model::{BranchRef, RepositoryIdentity, UtcInstant};

use super::{decode_branch_ref, decode_digest, decode_instant, decode_repository, root};

const TRUSTED_TIME_STATEMENT_SCHEMA: &str = "amiss/scanner-trusted-time-statement/v1";
const TRUSTED_TIME_CONTROLLER: &str = "github-actions-required-workflow-clock-v1";

/// The controller's maximum statement lifetime: `evaluation_instant <
/// valid_until <= evaluation_instant + 600` whole seconds.
pub const STATEMENT_TTL_MAX_SECONDS: i64 = 600;

/// A trusted-time statement issued by the required-workflow clock inside the
/// externally controlled run. Parsing establishes shape and the TTL law; the
/// evaluation-side bindings (repository, ref, candidate identity, run,
/// attempt) are separate verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedTimeStatement {
    pub digest: Digest,
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub candidate_identity_digest: Digest,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

impl TrustedTimeStatement {
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
        let run_id_path = obj.field("provider_run_id");
        let provider_run_id = de::string(&run_id_path, obj.take("provider_run_id")?)?;
        let run_id_bytes = provider_run_id.as_bytes();
        if run_id_bytes.is_empty()
            || run_id_bytes.len() > 32
            || !matches!(run_id_bytes.first(), Some(b'1'..=b'9'))
            || !run_id_bytes.iter().all(u8::is_ascii_digit)
        {
            return fail(&run_id_path, ErrorKind::InvalidValue);
        }
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
            provider_run_id,
            provider_run_attempt,
            evaluation_instant,
            valid_until,
        })
    }
}
