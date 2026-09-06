use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{TrustedTimeStatement, provider_run_id_valid, root};
use crate::de::{self, Error, ErrorKind};
use crate::digest::Digest;
use crate::model::ArtifactId;

mod candidate;
mod evaluation;

pub use candidate::{
    CandidateEventKind, CandidateFinality, CandidateIdentity, CandidateIdentitySchema,
    CandidateSnapshot, GitSnapshotIdentity, GitSnapshotKind, IndexIdentityScope,
    IndexSnapshotIdentity, IndexSnapshotKind, IndexSnapshotSchema,
    commit_candidate_identity_digest,
};
pub use evaluation::{EvaluationRequest, EvaluationRequestSchema};

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
    Display,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
    EnumString,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RequestMode {
    CommitPair,
    Index,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SnapshotSchema {
    #[strum(serialize = "amiss/scanner-snapshot-request")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SnapshotMaterialization {
    GitObjects,
    Index,
}

/// The materialization request. `git-objects` pairs with mode `commit-pair`
/// and `index` with mode `index`; the pairing law is checked against the
/// evaluation request by the consumer, since each request parses alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRequest {
    pub schema: SnapshotSchema,
    pub materialization: SnapshotMaterialization,
    pub repository_handle: i64,
    pub pre_acquired: bool,
}

impl SnapshotRequest {
    #[must_use]
    pub const fn git_objects() -> Self {
        Self {
            schema: SnapshotSchema::Current,
            materialization: SnapshotMaterialization::GitObjects,
            repository_handle: REPOSITORY_HANDLE_ORDINAL,
            pre_acquired: true,
        }
    }

    #[must_use]
    pub const fn index() -> Self {
        Self {
            schema: SnapshotSchema::Current,
            materialization: SnapshotMaterialization::Index,
            repository_handle: REPOSITORY_HANDLE_ORDINAL,
            pre_acquired: true,
        }
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        root(bytes)?;
        let request: Self = de::deserialize_json(bytes)?;
        validate_snapshot(&request)?;
        Ok(request)
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        validate_snapshot(self)?;
        serde_json_canonicalizer::to_vec(self)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))
    }
}

fn validate_snapshot(request: &SnapshotRequest) -> Result<(), Error> {
    (request.repository_handle == REPOSITORY_HANDLE_ORDINAL)
        .then_some(())
        .ok_or_else(|| Error::new("$.repository_handle", ErrorKind::InvalidValue))?;
    request
        .pre_acquired
        .then_some(())
        .ok_or_else(|| Error::new("$.pre_acquired", ErrorKind::InvalidValue))
}

/// One supplied external control: the exact embedded JSON value, the
/// independently acquired expected semantic digest, and the external trust
/// source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedControl {
    pub value: serde_json::Value,
    pub expected_digest: Digest,
    pub trust_source: RequestTrust,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
    EnumString,
    strum::IntoStaticStr,
    Display,
)]
#[strum(serialize_all = "kebab-case")]
pub enum RequestTrust {
    ExternalRequiredCheck,
    OrganizationPolicy,
}

/// The supplied trusted-time statement with the provider-authenticated run
/// context the statement must identify. Its trust source is fixed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedTime {
    #[serde(deserialize_with = "object::deserialize")]
    pub value: TrustedTimeStatement,
    pub expected_digest: Digest,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
}

// An empty prefix preserves every key while the library requires an object, not a sequence.
serde_with::with_prefix!(object "");

/// One semantic envelope paired with the independently planned build or
/// inventory context it must identify.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedSemanticEvidence {
    pub value: serde_json::Value,
    pub expected_context_digest: Digest,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum ControlsRequestSchema {
    #[default]
    #[strum(serialize = "amiss/scanner-controls-request")]
    Current,
}

/// The external-input request: five nullable supplied controls and the
/// bounded semantic-evidence set the trusted caller acquired.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlsRequest {
    pub schema: ControlsRequestSchema,
    #[serde(deserialize_with = "Option::deserialize")]
    pub organization_floor: Option<SuppliedControl>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub debt_snapshot: Option<SuppliedControl>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub waiver_bundle: Option<SuppliedControl>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub trusted_time: Option<SuppliedTime>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub execution_constraint: Option<SuppliedControl>,
    pub semantic_evidence: Vec<SuppliedSemanticEvidence>,
}

impl ControlsRequest {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values. Trusted time is decoded under its closed schema;
    /// the other embedded controls are shape-checked as objects only.
    /// Consumers verify semantic constraints and independent digests.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        root(bytes)?;
        let request: Self = de::deserialize_json(bytes)?;
        validate_controls(&request)?;
        Ok(request)
    }

    /// Serializes one valid request to its unique canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// The constructed fields violate the same laws [`Self::parse`] enforces.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Error> {
        validate_controls(self)?;
        let bytes = serde_json_canonicalizer::to_vec(self)
            .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
        root(&bytes)?;
        Ok(bytes)
    }
}

fn validate_controls(request: &ControlsRequest) -> Result<(), Error> {
    [
        ("$.organization_floor.value", &request.organization_floor),
        ("$.debt_snapshot.value", &request.debt_snapshot),
        ("$.waiver_bundle.value", &request.waiver_bundle),
        (
            "$.execution_constraint.value",
            &request.execution_constraint,
        ),
    ]
    .into_iter()
    .filter_map(|(path, supplied)| supplied.as_ref().map(|value| (path, &value.value)))
    .try_for_each(|(path, value)| require_object(path, value))?;

    if let Some(time) = &request.trusted_time {
        ArtifactId::new(time.provider.clone())
            .is_some()
            .then_some(())
            .ok_or_else(|| Error::new("$.trusted_time.provider", ErrorKind::InvalidValue))?;
        provider_run_id_valid(&time.provider_run_id)
            .then_some(())
            .ok_or_else(|| Error::new("$.trusted_time.provider_run_id", ErrorKind::InvalidValue))?;
        (1..=9_007_199_254_740_991)
            .contains(&time.provider_run_attempt)
            .then_some(())
            .ok_or_else(|| {
                Error::new(
                    "$.trusted_time.provider_run_attempt",
                    ErrorKind::InvalidValue,
                )
            })?;
    }

    if request.semantic_evidence.len() > SEMANTIC_EVIDENCE_REQUEST_LIMIT {
        return Err(Error::new("$.semantic_evidence", ErrorKind::LimitExceeded));
    }
    request
        .semantic_evidence
        .iter()
        .enumerate()
        .try_for_each(|(index, evidence)| {
            require_object(
                &format!("$.semantic_evidence[{index}].value"),
                &evidence.value,
            )
        })
}

fn require_object(path: &str, value: &serde_json::Value) -> Result<(), Error> {
    value
        .is_object()
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::WrongType))
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
