use std::io::{Read, Write};

use crate::controls::value::{object, positive_safe_integer, text};
use crate::controls::{decode_enum, decode_provider_id, decode_provider_run_id, root};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::Digest;
use crate::json::{Value, canonical};

mod evaluation;

pub use evaluation::{EvaluationRequest, commit_candidate_identity_digest};

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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RequestMode {
    CommitPair,
    Index,
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
        obj.required("schema", |path, value| {
            de::const_str(path, value, SNAPSHOT_REQUEST_SCHEMA)
        })?;
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RequestTrust {
    ExternalRequiredCheck,
    OrganizationPolicy,
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
        obj.required("schema", |path, value| {
            de::const_str(path, value, CONTROLS_REQUEST_SCHEMA)
        })?;
        let organization_floor = obj.required("organization_floor", decode_supplied)?;
        let debt_snapshot = obj.required("debt_snapshot", decode_supplied)?;
        let waiver_bundle = obj.required("waiver_bundle", decode_supplied)?;
        let trusted_time = obj.required("trusted_time", decode_time)?;
        let execution_constraint = obj.required("execution_constraint", decode_supplied)?;
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
        checked_canonical(&controls_value(self)?, Self::parse)
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
    rows.push(("trust_source", text(control.trust_source.as_ref())));
    object(rows)
}

fn supplied_time_value(time: &SuppliedTime) -> Result<Value, Error> {
    let mut rows = supplied_rows(&time.value, time.expected_digest);
    rows.extend([
        ("provider", text(&time.provider)),
        ("provider_run_id", text(&time.provider_run_id)),
        (
            "provider_run_attempt",
            positive_safe_integer(
                "$.trusted_time.provider_run_attempt",
                time.provider_run_attempt,
            )?,
        ),
    ]);
    Ok(object(rows))
}

fn supplied_rows(value: &Value, expected_digest: Digest) -> Vec<(&'static str, Value)> {
    vec![
        ("value", value.clone()),
        ("expected_digest", text(&expected_digest.to_string())),
    ]
}

fn controls_value(request: &ControlsRequest) -> Result<Value, Error> {
    let mut rows = Vec::with_capacity(6);
    for (name, control) in [
        ("organization_floor", request.organization_floor.as_ref()),
        ("debt_snapshot", request.debt_snapshot.as_ref()),
        ("waiver_bundle", request.waiver_bundle.as_ref()),
    ] {
        rows.push((name, optional_supplied(control)));
    }
    let trusted_time = request
        .trusted_time
        .as_ref()
        .map(supplied_time_value)
        .transpose()?
        .unwrap_or(Value::Null);
    rows.push(("trusted_time", trusted_time));
    rows.push((
        "execution_constraint",
        optional_supplied(request.execution_constraint.as_ref()),
    ));
    rows.push(("schema", text(CONTROLS_REQUEST_SCHEMA)));
    Ok(object(rows))
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
    let embedded = obj.required("value", embedded_value)?;
    let digest_path = obj.field("expected_digest");
    let expected_digest = de::digest(&digest_path, obj.take("expected_digest")?)?;
    let trust_source = obj.required("trust_source", decode_enum)?;
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
    let embedded = obj.required("value", embedded_value)?;
    let digest_path = obj.field("expected_digest");
    let expected_digest = de::digest(&digest_path, obj.take("expected_digest")?)?;
    let provider = obj.required("provider", decode_provider_id)?;
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
