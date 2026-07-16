use crate::controls::{
    Profile, decode_provider_id, decode_provider_run_id, decode_repository, root,
};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

pub const EVALUATION_REQUEST_SCHEMA: &str = "amiss/scanner-evaluation-request";
pub const SNAPSHOT_REQUEST_SCHEMA: &str = "amiss/scanner-snapshot-request";
pub const CONTROLS_REQUEST_SCHEMA: &str = "amiss/scanner-controls-request";

/// Every request stream is one complete bounded byte capture from byte zero
/// through EOF; its diagnostic digest exists exactly when EOF was obtained
/// within this cap.
pub const REQUEST_STREAM_BYTES: u64 = 16_777_216;

/// The published handle table's repository ordinal, constant across the
/// in-process and future subprocess lanes.
pub const REPOSITORY_HANDLE_ORDINAL: i64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestMode {
    CommitPair,
    Index,
}

impl RequestMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommitPair => "commit-pair",
            Self::Index => "index",
        }
    }
}

/// The run-identity request: profile, mode, and the exact snapshot
/// identities to evaluate. The candidate commit is null exactly when the
/// mode is `index`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationRequest {
    pub profile: Profile,
    pub mode: RequestMode,
    pub object_format: ObjectFormat,
    pub repository: Option<RepositoryIdentity>,
    pub forge: Option<ForgeDialect>,
    pub ref_name: Option<BranchRef>,
    pub default_branch_ref: Option<BranchRef>,
    pub base_commit: Oid,
    pub candidate_commit: Option<Oid>,
}

impl EvaluationRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid
    /// grammar values, and a candidate commit inconsistent with the mode.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            EVALUATION_REQUEST_SCHEMA,
        )?;
        let profile = Profile::decode(&obj.field("profile"), obj.take("profile")?)?;
        let mode_path = obj.field("mode");
        let mode = match de::string(&mode_path, obj.take("mode")?)?.as_str() {
            "commit-pair" => RequestMode::CommitPair,
            "index" => RequestMode::Index,
            _ => return fail(&mode_path, ErrorKind::InvalidValue),
        };
        let format_path = obj.field("object_format");
        let object_format = match de::string(&format_path, obj.take("object_format")?)?.as_str() {
            "sha1" => ObjectFormat::Sha1,
            "sha256" => ObjectFormat::Sha256,
            _ => return fail(&format_path, ErrorKind::InvalidValue),
        };
        let repository_path = obj.field("repository");
        let repository = match de::nullable(obj.take("repository")?) {
            None => None,
            Some(value) => Some(decode_repository(&repository_path, value)?),
        };
        let forge_path = obj.field("forge");
        let forge = de::nullable(obj.take("forge")?)
            .map(|value| decode_forge(&forge_path, value))
            .transpose()?;
        let ref_path = obj.field("ref");
        let ref_name = match de::nullable(obj.take("ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&ref_path, value)?),
        };
        let default_path = obj.field("default_branch_ref");
        let default_branch_ref = match de::nullable(obj.take("default_branch_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&default_path, value)?),
        };
        let base_path = obj.field("base_commit_oid");
        let base_commit = Oid::new(
            object_format,
            de::string(&base_path, obj.take("base_commit_oid")?)?,
        )
        .ok_or_else(|| Error::new(&base_path, ErrorKind::InvalidValue))?;
        let candidate_path = obj.field("candidate_commit_oid");
        let candidate_commit = match de::nullable(obj.take("candidate_commit_oid")?) {
            None => None,
            Some(value) => Some(
                Oid::new(object_format, de::string(&candidate_path, value)?)
                    .ok_or_else(|| Error::new(&candidate_path, ErrorKind::InvalidValue))?,
            ),
        };
        obj.finish()?;
        let consistent = match mode {
            RequestMode::CommitPair => candidate_commit.is_some(),
            RequestMode::Index => candidate_commit.is_none(),
        };
        if !consistent {
            return fail(&candidate_path, ErrorKind::Inconsistent);
        }
        if forge.is_some() && repository.is_none()
            || matches!(forge, Some(ForgeDialect::Github | ForgeDialect::Gitea))
                && repository
                    .as_ref()
                    .is_some_and(|identity| identity.owner.contains('/'))
        {
            return fail(&forge_path, ErrorKind::Inconsistent);
        }
        Ok(Self {
            profile,
            mode,
            object_format,
            repository,
            forge,
            ref_name,
            default_branch_ref,
            base_commit,
            candidate_commit,
        })
    }
}

fn decode_forge(path: &str, value: Value) -> Result<ForgeDialect, Error> {
    match de::string(path, value)?.as_str() {
        "github" => Ok(ForgeDialect::Github),
        "gitlab" => Ok(ForgeDialect::Gitlab),
        "gitea" => Ok(ForgeDialect::Gitea),
        _ => fail(path, ErrorKind::InvalidValue),
    }
}

fn decode_ref(path: &str, value: Value) -> Result<BranchRef, Error> {
    BranchRef::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

/// The materialization request. `git-objects` pairs with mode `commit-pair`
/// and `index` with mode `index`; the pairing law is checked against the
/// evaluation request by the consumer, since each request parses alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub materialization: RequestMode,
}

impl SnapshotRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            SNAPSHOT_REQUEST_SCHEMA,
        )?;
        let materialization_path = obj.field("materialization");
        let materialization =
            match de::string(&materialization_path, obj.take("materialization")?)?.as_str() {
                "git-objects" => RequestMode::CommitPair,
                "index" => RequestMode::Index,
                _ => return fail(&materialization_path, ErrorKind::InvalidValue),
            };
        let handle_path = obj.field("repository_handle");
        if de::integer(&handle_path, obj.take("repository_handle")?)? != REPOSITORY_HANDLE_ORDINAL {
            return fail(&handle_path, ErrorKind::InvalidValue);
        }
        let acquired_path = obj.field("pre_acquired");
        if obj.take("pre_acquired")? != Value::Bool(true) {
            return fail(&acquired_path, ErrorKind::InvalidValue);
        }
        obj.finish()?;
        Ok(Self { materialization })
    }
}

/// One supplied external control: the exact embedded JSON value, the
/// independently acquired expected semantic digest, and the external trust
/// source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedControl {
    pub value: Value,
    pub expected_digest: Digest,
    pub trust_source: RequestTrust,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestTrust {
    ExternalRequiredCheck,
    OrganizationPolicy,
}

impl RequestTrust {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExternalRequiredCheck => "external-required-check",
            Self::OrganizationPolicy => "organization-policy",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "external-required-check" => Ok(Self::ExternalRequiredCheck),
            "organization-policy" => Ok(Self::OrganizationPolicy),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

/// The supplied trusted-time statement with the provider-authenticated run
/// context the statement must identify. Its trust source is fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuppliedTime {
    pub value: Value,
    pub expected_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
}

/// The external-control request: five nullable supplied controls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlsRequest {
    pub organization_floor: Option<SuppliedControl>,
    pub debt_snapshot: Option<SuppliedControl>,
    pub waiver_bundle: Option<SuppliedControl>,
    pub trusted_time: Option<SuppliedTime>,
    pub execution_constraint: Option<SuppliedControl>,
}

impl ControlsRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values. Embedded control values are shape-checked as objects
    /// only; their own schemas and digests are the consumer's verification.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            CONTROLS_REQUEST_SCHEMA,
        )?;
        let organization_floor = decode_supplied(
            &obj.field("organization_floor"),
            obj.take("organization_floor")?,
        )?;
        let debt_snapshot =
            decode_supplied(&obj.field("debt_snapshot"), obj.take("debt_snapshot")?)?;
        let waiver_bundle =
            decode_supplied(&obj.field("waiver_bundle"), obj.take("waiver_bundle")?)?;
        let trusted_time = decode_time(&obj.field("trusted_time"), obj.take("trusted_time")?)?;
        let execution_constraint = decode_supplied(
            &obj.field("execution_constraint"),
            obj.take("execution_constraint")?,
        )?;
        obj.finish()?;
        Ok(Self {
            organization_floor,
            debt_snapshot,
            waiver_bundle,
            trusted_time,
            execution_constraint,
        })
    }
}

fn embedded_value(path: &str, value: Value) -> Result<Value, Error> {
    match value {
        Value::Object(_) => Ok(value),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Array(_) => {
            fail(path, ErrorKind::WrongType)
        }
    }
}

fn decode_supplied(path: &str, value: Value) -> Result<Option<SuppliedControl>, Error> {
    let Some(value) = de::nullable(value) else {
        return Ok(None);
    };
    let mut obj = Obj::new(path, value)?;
    let embedded = embedded_value(&obj.field("value"), obj.take("value")?)?;
    let digest_path = obj.field("expected_digest");
    let expected_digest =
        Digest::from_wire(&de::string(&digest_path, obj.take("expected_digest")?)?)
            .ok_or_else(|| Error::new(&digest_path, ErrorKind::InvalidValue))?;
    let trust_source = RequestTrust::decode(&obj.field("trust_source"), obj.take("trust_source")?)?;
    obj.finish()?;
    Ok(Some(SuppliedControl {
        value: embedded,
        expected_digest,
        trust_source,
    }))
}

fn decode_time(path: &str, value: Value) -> Result<Option<SuppliedTime>, Error> {
    let Some(value) = de::nullable(value) else {
        return Ok(None);
    };
    let mut obj = Obj::new(path, value)?;
    let embedded = embedded_value(&obj.field("value"), obj.take("value")?)?;
    let digest_path = obj.field("expected_digest");
    let expected_digest =
        Digest::from_wire(&de::string(&digest_path, obj.take("expected_digest")?)?)
            .ok_or_else(|| Error::new(&digest_path, ErrorKind::InvalidValue))?;
    let provider = decode_provider_id(&obj.field("provider"), obj.take("provider")?)?;
    let run_id_path = obj.field("provider_run_id");
    let provider_run_id = decode_provider_run_id(&run_id_path, obj.take("provider_run_id")?)?;
    let attempt_path = obj.field("provider_run_attempt");
    let attempt_raw = de::integer(&attempt_path, obj.take("provider_run_attempt")?)?;
    let provider_run_attempt = u64::try_from(attempt_raw)
        .ok()
        .filter(|attempt| *attempt >= 1)
        .ok_or_else(|| Error::new(&attempt_path, ErrorKind::InvalidValue))?;
    obj.finish()?;
    Ok(Some(SuppliedTime {
        value: embedded,
        expected_digest,
        provider,
        provider_run_id,
        provider_run_attempt,
    }))
}
