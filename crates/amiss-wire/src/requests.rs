use std::io::{Read, Write};

use crate::controls::{
    Profile, decode_provider_id, decode_provider_run_id, decode_repository, root,
};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::{Value, canonical};
use crate::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

pub const EVALUATION_REQUEST_SCHEMA: &str = "amiss/scanner-evaluation-request";
pub const SNAPSHOT_REQUEST_SCHEMA: &str = "amiss/scanner-snapshot-request";
pub const CONTROLS_REQUEST_SCHEMA: &str = "amiss/scanner-controls-request";
pub const CANDIDATE_IDENTITY_DOMAIN: &str = "amiss/scanner-candidate-identity";

/// The one non-public engine entry point the trusted bootstrap invokes. The
/// ordinary command grammar never recognizes this argument.
pub const SEALED_ENGINE_ARGUMENT: &str = "__amiss-sealed-request-v1";

const SEALED_FRAME_MAGIC: &[u8; 8] = b"AMISSRQ1";

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
    pub candidate_ref: Option<BranchRef>,
    pub target_ref: Option<BranchRef>,
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
        let candidate_ref_path = obj.field("candidate_ref");
        let candidate_ref = match de::nullable(obj.take("candidate_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&candidate_ref_path, value)?),
        };
        let target_ref_path = obj.field("target_ref");
        let target_ref = match de::nullable(obj.take("target_ref")?) {
            None => None,
            Some(value) => Some(decode_ref(&target_ref_path, value)?),
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
        let identity_fields = [
            repository.is_some(),
            candidate_ref.is_some(),
            target_ref.is_some(),
            default_branch_ref.is_some(),
        ];
        if !identity_fields.iter().all(|present| *present)
            && identity_fields.iter().any(|present| *present)
            || forge.is_some() && repository.is_none()
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
            candidate_ref,
            target_ref,
            default_branch_ref,
            base_commit,
            candidate_commit,
        })
    }

    /// Builds an explicit-commit evaluation with no forge identity. Callers
    /// may then fill the public identity fields before serialization.
    #[must_use]
    pub fn commit_pair(
        profile: Profile,
        object_format: ObjectFormat,
        base_commit: Oid,
        candidate_commit: Oid,
    ) -> Self {
        Self::without_identity(profile, object_format, base_commit, Some(candidate_commit))
    }

    /// Builds a staged-index evaluation with no forge identity.
    #[must_use]
    pub fn index(profile: Profile, object_format: ObjectFormat, base_commit: Oid) -> Self {
        Self::without_identity(profile, object_format, base_commit, None)
    }

    fn without_identity(
        profile: Profile,
        object_format: ObjectFormat,
        base_commit: Oid,
        candidate_commit: Option<Oid>,
    ) -> Self {
        Self {
            profile,
            mode: if candidate_commit.is_some() {
                RequestMode::CommitPair
            } else {
                RequestMode::Index
            },
            object_format,
            repository: None,
            forge: None,
            candidate_ref: None,
            target_ref: None,
            default_branch_ref: None,
            base_commit,
            candidate_commit,
        }
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        checked_canonical(&evaluation_value(self), Self::parse)
    }
}

fn decode_forge(path: &str, value: Value) -> Result<ForgeDialect, Error> {
    let raw = de::string(path, value)?;
    raw.parse()
        .map_err(|_unknown| Error::new(path, ErrorKind::InvalidValue))
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
    #[must_use]
    pub const fn git_objects() -> Self {
        Self {
            materialization: RequestMode::CommitPair,
        }
    }

    #[must_use]
    pub const fn index() -> Self {
        Self {
            materialization: RequestMode::Index,
        }
    }

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

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        checked_canonical(&snapshot_value(*self), Self::parse)
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

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        if self
            .trusted_time
            .as_ref()
            .is_some_and(|time| i64::try_from(time.provider_run_attempt).is_err())
        {
            return fail(
                "$.trusted_time.provider_run_attempt",
                ErrorKind::InvalidValue,
            );
        }
        checked_canonical(&controls_value(self), Self::parse)
    }
}

/// The three exact streams carried through the bootstrap-to-engine pipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestStreams {
    pub evaluation: Vec<u8>,
    pub snapshot: Vec<u8>,
    pub controls: Vec<u8>,
}

impl RequestStreams {
    /// Writes the closed frame: magic, then three big-endian lengths and
    /// their exact request bytes in evaluation/snapshot/controls order.
    ///
    /// # Errors
    ///
    /// A stream exceeds the request ceiling or the destination cannot be
    /// written completely.
    pub fn write_to(&self, writer: &mut impl Write) -> std::io::Result<()> {
        writer.write_all(SEALED_FRAME_MAGIC)?;
        for bytes in [&self.evaluation, &self.snapshot, &self.controls] {
            let length = u64::try_from(bytes.len())
                .map_err(|_defect| invalid_frame("request length is not representable"))?;
            if length > REQUEST_STREAM_BYTES {
                return Err(invalid_frame("request exceeds the stream ceiling"));
            }
            writer.write_all(&length.to_be_bytes())?;
            writer.write_all(bytes)?;
        }
        Ok(())
    }

    /// Reads one complete closed request frame and refuses trailing bytes.
    ///
    /// # Errors
    ///
    /// The source is truncated, malformed, oversized, has trailing bytes,
    /// or otherwise cannot be read completely.
    pub fn read_from(reader: &mut impl Read) -> std::io::Result<Self> {
        let mut magic = [0_u8; 8];
        reader.read_exact(&mut magic)?;
        if &magic != SEALED_FRAME_MAGIC {
            return Err(invalid_frame("wrong sealed request frame"));
        }
        let evaluation = read_stream(reader)?;
        let snapshot = read_stream(reader)?;
        let controls = read_stream(reader)?;
        let mut trailing = [0_u8; 1];
        if reader.read(&mut trailing)? != 0 {
            return Err(invalid_frame("trailing sealed request bytes"));
        }
        Ok(Self {
            evaluation,
            snapshot,
            controls,
        })
    }
}

fn read_stream(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut encoded = [0_u8; 8];
    reader.read_exact(&mut encoded)?;
    let length = u64::from_be_bytes(encoded);
    if length > REQUEST_STREAM_BYTES {
        return Err(invalid_frame("request exceeds the stream ceiling"));
    }
    let capacity = usize::try_from(length)
        .map_err(|_defect| invalid_frame("request length is not representable"))?;
    let mut bytes = vec![0_u8; capacity];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn invalid_frame(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn checked_canonical<T>(
    value: &Value,
    parse: impl FnOnce(&[u8]) -> Result<T, Error>,
) -> Result<Vec<u8>, Error> {
    let bytes = canonical(value);
    let _parsed = parse(&bytes)?;
    Ok(bytes)
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn optional_text(value: Option<&str>) -> Value {
    value.map_or(Value::Null, text)
}

fn object(rows: Vec<(&str, Value)>) -> Value {
    Value::Object(
        rows.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn repository_value(identity: &RepositoryIdentity) -> Value {
    object(vec![
        ("host", text(&identity.host)),
        ("owner", text(&identity.owner)),
        ("name", text(&identity.name)),
    ])
}

fn evaluation_value(request: &EvaluationRequest) -> Value {
    object(vec![
        ("schema", text(EVALUATION_REQUEST_SCHEMA)),
        (
            "profile",
            text(match request.profile {
                Profile::Observe => "observe",
                Profile::Enforce => "enforce",
            }),
        ),
        ("mode", text(request.mode.as_str())),
        ("object_format", text(request.object_format.as_str())),
        (
            "repository",
            request
                .repository
                .as_ref()
                .map_or(Value::Null, repository_value),
        ),
        (
            "forge",
            request
                .forge
                .map_or(Value::Null, |forge| text(forge.as_str())),
        ),
        (
            "candidate_ref",
            optional_text(request.candidate_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "target_ref",
            optional_text(request.target_ref.as_ref().map(BranchRef::as_str)),
        ),
        (
            "default_branch_ref",
            optional_text(request.default_branch_ref.as_ref().map(BranchRef::as_str)),
        ),
        ("base_commit_oid", text(request.base_commit.as_str())),
        (
            "candidate_commit_oid",
            optional_text(request.candidate_commit.as_ref().map(Oid::as_str)),
        ),
    ])
}

fn snapshot_value(request: SnapshotRequest) -> Value {
    object(vec![
        ("schema", text(SNAPSHOT_REQUEST_SCHEMA)),
        (
            "materialization",
            text(match request.materialization {
                RequestMode::CommitPair => "git-objects",
                RequestMode::Index => "index",
            }),
        ),
        (
            "repository_handle",
            Value::Integer(REPOSITORY_HANDLE_ORDINAL),
        ),
        ("pre_acquired", Value::Bool(true)),
    ])
}

fn supplied_value(control: &SuppliedControl) -> Value {
    let mut rows = supplied_rows(&control.value, control.expected_digest);
    rows.push(("trust_source", text(control.trust_source.as_str())));
    object(rows)
}

fn supplied_time_value(time: &SuppliedTime) -> Value {
    let mut rows = supplied_rows(&time.value, time.expected_digest);
    rows.extend([
        ("provider", text(&time.provider)),
        ("provider_run_id", text(&time.provider_run_id)),
        (
            "provider_run_attempt",
            Value::Integer(i64::try_from(time.provider_run_attempt).unwrap_or(i64::MAX)),
        ),
    ]);
    object(rows)
}

fn supplied_rows(value: &Value, expected_digest: Digest) -> Vec<(&'static str, Value)> {
    vec![
        ("value", value.clone()),
        ("expected_digest", text(&expected_digest.to_string())),
    ]
}

fn controls_value(request: &ControlsRequest) -> Value {
    let mut rows = Vec::with_capacity(6);
    for (name, control) in [
        ("organization_floor", request.organization_floor.as_ref()),
        ("debt_snapshot", request.debt_snapshot.as_ref()),
        ("waiver_bundle", request.waiver_bundle.as_ref()),
    ] {
        rows.push((name, optional_supplied(control)));
    }
    rows.push((
        "trusted_time",
        request
            .trusted_time
            .as_ref()
            .map_or(Value::Null, supplied_time_value),
    ));
    rows.push((
        "execution_constraint",
        optional_supplied(request.execution_constraint.as_ref()),
    ));
    rows.push(("schema", text(CONTROLS_REQUEST_SCHEMA)));
    object(rows)
}

fn optional_supplied(control: Option<&SuppliedControl>) -> Value {
    control.map_or(Value::Null, supplied_value)
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
