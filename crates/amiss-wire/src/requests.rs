use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::codec::{self, nullable};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;

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
pub const SEMANTIC_EVIDENCE_REQUEST_LIMIT: usize = 64;

/// The published handle table's repository ordinal, constant across the
/// in-process and future subprocess lanes.
pub const REPOSITORY_HANDLE_ORDINAL: i64 = 3;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::EnumString,
    strum::IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
        let request: SnapshotDocument = decode_request(bytes)?;
        if request.schema != SNAPSHOT_REQUEST_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        if request.repository_handle != REPOSITORY_HANDLE_ORDINAL {
            return fail("$.repository_handle", ErrorKind::InvalidValue);
        }
        if !request.pre_acquired {
            return fail("$.pre_acquired", ErrorKind::InvalidValue);
        }
        Ok(Self {
            materialization: match request.materialization {
                Materialization::GitObjects => RequestMode::CommitPair,
                Materialization::Index => RequestMode::Index,
            },
        })
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        request_bytes(&SnapshotDocument {
            schema: SNAPSHOT_REQUEST_SCHEMA.to_owned(),
            materialization: match self.materialization {
                RequestMode::CommitPair => Materialization::GitObjects,
                RequestMode::Index => Materialization::Index,
            },
            repository_handle: REPOSITORY_HANDLE_ORDINAL,
            pre_acquired: true,
        })
    }
}

/// One supplied external control: the exact embedded JSON value, the
/// independently acquired expected semantic digest, and the external trust
/// source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedControl {
    pub value: Value,
    pub expected_digest: Digest,
    pub trust_source: RequestTrust,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::EnumString,
    strum::IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum RequestTrust {
    ExternalRequiredCheck,
    OrganizationPolicy,
}

/// The supplied trusted-time statement with the provider-authenticated run
/// context the statement must identify. Its trust source is fixed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedTime {
    pub value: Value,
    pub expected_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
}

/// One semantic envelope paired with the independently planned build or
/// inventory context it must identify.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedSemanticEvidence {
    pub value: Value,
    pub expected_context_digest: Digest,
}

/// The external-input request: five nullable supplied controls and the
/// bounded semantic-evidence set the trusted caller acquired.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlsRequest {
    #[serde(deserialize_with = "nullable")]
    pub organization_floor: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    pub debt_snapshot: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    pub waiver_bundle: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    pub trusted_time: Option<SuppliedTime>,
    #[serde(deserialize_with = "nullable")]
    pub execution_constraint: Option<SuppliedControl>,
    pub semantic_evidence: Vec<SuppliedSemanticEvidence>,
}

impl ControlsRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values. Embedded control values are shape-checked as objects
    /// only; their own schemas and digests are the consumer's verification.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let request: ControlsDocument = decode_request(bytes)?;
        if request.schema != CONTROLS_REQUEST_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        let controls = Self {
            organization_floor: request.organization_floor,
            debt_snapshot: request.debt_snapshot,
            waiver_bundle: request.waiver_bundle,
            trusted_time: request.trusted_time,
            execution_constraint: request.execution_constraint,
            semantic_evidence: request.semantic_evidence,
        };
        controls.validate()?;
        Ok(controls)
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        request_bytes(&ControlsOutput {
            schema: CONTROLS_REQUEST_SCHEMA,
            controls: self,
        })
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDocument {
    schema: String,
    materialization: Materialization,
    repository_handle: i64,
    pre_acquired: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Materialization {
    GitObjects,
    Index,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlsDocument {
    schema: String,
    #[serde(deserialize_with = "nullable")]
    organization_floor: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    debt_snapshot: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    waiver_bundle: Option<SuppliedControl>,
    #[serde(deserialize_with = "nullable")]
    trusted_time: Option<SuppliedTime>,
    #[serde(deserialize_with = "nullable")]
    execution_constraint: Option<SuppliedControl>,
    semantic_evidence: Vec<SuppliedSemanticEvidence>,
}

#[derive(Serialize)]
struct ControlsOutput<'a> {
    schema: &'static str,
    #[serde(flatten)]
    controls: &'a ControlsRequest,
}

impl ControlsRequest {
    fn validate(&self) -> Result<(), Error> {
        for (name, supplied) in [
            ("organization_floor", &self.organization_floor),
            ("debt_snapshot", &self.debt_snapshot),
            ("waiver_bundle", &self.waiver_bundle),
            ("execution_constraint", &self.execution_constraint),
        ] {
            if let Some(supplied) = supplied {
                embedded_object(&format!("$.{name}.value"), &supplied.value)?;
            }
        }
        if let Some(time) = &self.trusted_time {
            embedded_object("$.trusted_time.value", &time.value)?;
            if ArtifactId::new(time.provider.clone()).is_none() {
                return fail("$.trusted_time.provider", ErrorKind::InvalidValue);
            }
            let run = time.provider_run_id.as_bytes();
            if run.is_empty()
                || run.len() > 128
                || !run.first().is_some_and(u8::is_ascii_alphanumeric)
                || !run.last().is_some_and(u8::is_ascii_alphanumeric)
                || !run.iter().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
                })
            {
                return fail("$.trusted_time.provider_run_id", ErrorKind::InvalidValue);
            }
            if !(1..=codec::MAX_SAFE_INTEGER).contains(&time.provider_run_attempt) {
                return fail(
                    "$.trusted_time.provider_run_attempt",
                    ErrorKind::InvalidValue,
                );
            }
        }
        if self.semantic_evidence.len() > SEMANTIC_EVIDENCE_REQUEST_LIMIT {
            return fail("$.semantic_evidence", ErrorKind::LimitExceeded);
        }
        for (index, supplied) in self.semantic_evidence.iter().enumerate() {
            embedded_object(
                &format!("$.semantic_evidence[{index}].value"),
                &supplied.value,
            )?;
        }
        Ok(())
    }
}

fn embedded_object(path: &str, value: &Value) -> Result<(), Error> {
    if matches!(value, Value::Object(_)) {
        Ok(())
    } else {
        fail(path, ErrorKind::WrongType)
    }
}

fn decode_request<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    codec::decode(bytes)
}

fn request_bytes<T: Serialize>(request: &T) -> Result<Vec<u8>, Error> {
    let bytes = codec::canonical(request)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    Ok(bytes)
}
