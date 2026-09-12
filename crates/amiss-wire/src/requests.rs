use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, OrganizationFloor, TrustedTimeStatement,
    WaiverBundle, provider_run_id_valid,
};
use crate::de::{self, Error, ErrorKind};
use crate::model::ArtifactId;
use crate::model::Digest;
use crate::semantic::SemanticEvidenceEnvelope;

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

    /// Checks the fixed handle and prior repository acquisition required by the launcher.
    ///
    /// # Errors
    /// Refuses a different repository handle or a request to acquire the repository.
    pub fn validate(&self) -> Result<(), Error> {
        (self.repository_handle == REPOSITORY_HANDLE_ORDINAL)
            .then_some(())
            .ok_or_else(|| Error::new("$.repository_handle", ErrorKind::InvalidValue))?;
        self.pre_acquired
            .then_some(())
            .ok_or_else(|| Error::new("$.pre_acquired", ErrorKind::InvalidValue))
    }
}

/// One supplied external control: its concrete typed payload, the
/// independently acquired expected semantic digest, and the external trust
/// source that authorized it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedControl<T> {
    #[serde(bound(deserialize = "T: Deserialize<'de>"))]
    pub value: T,
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
    pub value: TrustedTimeStatement,
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
    pub value: SemanticEvidenceEnvelope<'static>,
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
    pub organization_floor: Option<SuppliedControl<OrganizationFloor>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub debt_snapshot: Option<SuppliedControl<DebtSnapshot>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub waiver_bundle: Option<SuppliedControl<WaiverBundle>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub trusted_time: Option<SuppliedTime>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub execution_constraint: Option<SuppliedControl<ExecutionConstraintDescriptor>>,
    pub semantic_evidence: Vec<SuppliedSemanticEvidence>,
}

impl ControlsRequest {
    /// # Errors
    ///
    /// Fails on JSON defects, schema-shape violations, and invalid
    /// grammar values. Controls and semantic evidence decode under their closed schemas.
    /// Consumers verify semantic constraints and independent digests.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let request: Self = serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|defect| de::deserialize_error("$", &defect))?;
        deserializer
            .end()
            .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;

        request.validate()?;
        Ok(request)
    }

    /// Checks request grammar, numeric bounds, and the semantic evidence count.
    /// Nested control meaning and independent digests remain checks for the consumer.
    ///
    /// # Errors
    /// Refuses malformed provider context, unsafe integers, or too many evidence envelopes.
    pub fn validate(&self) -> Result<(), Error> {
        if let Some(time) = &self.trusted_time {
            ArtifactId::new(time.provider.clone())
                .is_some()
                .then_some(())
                .ok_or_else(|| Error::new("$.trusted_time.provider", ErrorKind::InvalidValue))?;
            provider_run_id_valid(&time.provider_run_id)
                .then_some(())
                .ok_or_else(|| {
                    Error::new("$.trusted_time.provider_run_id", ErrorKind::InvalidValue)
                })?;
            (1..=js_int::MAX_SAFE_INT.unsigned_abs())
                .contains(&time.provider_run_attempt)
                .then_some(())
                .ok_or_else(|| {
                    Error::new(
                        "$.trusted_time.provider_run_attempt",
                        ErrorKind::InvalidValue,
                    )
                })?;
        }

        if self.semantic_evidence.len() > SEMANTIC_EVIDENCE_REQUEST_LIMIT {
            return Err(Error::new("$.semantic_evidence", ErrorKind::LimitExceeded));
        }
        if let Some(time) = &self.trusted_time {
            js_int::UInt::try_from(time.value.provider_run_attempt).map_err(|_defect| {
                Error::new(
                    "$.trusted_time.value.provider_run_attempt",
                    ErrorKind::InvalidValue,
                )
            })?;
        }
        if let Some(floor) = &self.organization_floor {
            for (index, limit) in floor.value.resource_limits.iter().enumerate() {
                js_int::Int::try_from(limit.maximum).map_err(|_defect| {
                    Error::new(
                        &format!("$.organization_floor.value.resource_limits[{index}].maximum"),
                        ErrorKind::InvalidValue,
                    )
                })?;
            }
        }
        let debt = self.debt_snapshot.iter().flat_map(|snapshot| {
            snapshot
                .value
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    (
                        "debt_snapshot",
                        "accepted_fact",
                        index,
                        item.accepted_fact.evidence.occurrence_multiplicity,
                    )
                })
        });
        let waivers = self.waiver_bundle.iter().flat_map(|bundle| {
            bundle.value.items.iter().enumerate().map(|(index, item)| {
                (
                    "waiver_bundle",
                    "authorized_fact",
                    index,
                    item.authorized_fact.evidence.occurrence_multiplicity,
                )
            })
        });
        for (control, fact, index, multiplicity) in debt.chain(waivers) {
            js_int::UInt::try_from(multiplicity).map_err(|_defect| {
                Error::new(
                    &format!(
                        "$.{control}.value.items[{index}].{fact}.evidence.occurrence_multiplicity"
                    ),
                    ErrorKind::InvalidValue,
                )
            })?;
        }
        Ok(())
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
