use std::{cmp::Ordering, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::assessment::Nullable;
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hj_serde};
use crate::json;
use crate::model::ArtifactId;

pub mod observation;
pub mod record;

pub const ENVELOPE_SCHEMA: &str = "amiss/semantic-evidence-envelope";
pub const PAYLOAD_SCHEMA: &str = "amiss/semantic-evidence-payload";
pub const TEMPLATE_SCHEMA: &str = "amiss/semantic-evidence-template";
pub const SEMANTIC_EVIDENCE_BYTES: u64 = 16_777_216;
pub const SEMANTIC_OBSERVATIONS_LIMIT: usize = 100_000;
pub const PRODUCER_VERSION_BYTES: usize = 128;
pub const RECORD_KEY_BYTES: usize = 4_096;
pub const RECORD_VALUE_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidenceEnvelope<O = serde_json::Value> {
    pub schema: EnvelopeSchema,
    pub payload: SemanticEvidence<O>,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeSchema {
    #[serde(rename = "amiss/semantic-evidence-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidence<O = serde_json::Value> {
    pub schema: PayloadSchema,
    pub subject: SemanticSubject,
    pub producer: SemanticProducer,
    pub complete: bool,
    pub observations: Vec<O>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PayloadSchema {
    #[serde(rename = "amiss/semantic-evidence-payload")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSubject {
    pub candidate_identity_digest: Digest,
    pub source_report_payload_digest: Nullable<Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProducer {
    pub kind: ArtifactId,
    pub identity: ArtifactId,
    pub version: String,
    pub context_digest: Digest,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidenceTemplate<O = serde_json::Value> {
    pub schema: TemplateSchema,
    pub producer: SemanticProducer,
    pub complete: bool,
    pub observations: Arc<[O]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateSchema {
    #[serde(rename = "amiss/semantic-evidence-template")]
    Current,
}

/// Parses one complete, digest-bound semantic evidence envelope.
///
/// # Errors
///
/// Fails on an oversized stream, strict-JSON or shape defects, a payload digest mismatch,
/// invalid producer identity, or observations that are not bounded sorted objects with kinds.
pub fn parse(bytes: &[u8]) -> Result<SemanticEvidenceEnvelope, Error> {
    let document: SemanticEvidenceEnvelope = parse_document(bytes)?;
    validate(&document)?;
    Ok(document)
}

/// Checks the digest and semantic laws of a decoded evidence envelope.
/// Readers enforce byte limits and strict JSON before decoding.
///
/// # Errors
///
/// Fails on a digest mismatch, invalid producer identity, or invalid observation set.
pub fn validate(document: &SemanticEvidenceEnvelope) -> Result<(), Error> {
    if payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    validate_producer("$.payload.producer", &document.payload.producer)?;
    validate_observations("$.payload.observations", &document.payload.observations)
}

/// Parses one candidate-independent semantic evidence template.
///
/// # Errors
///
/// Fails on an oversized stream, strict-JSON or shape defect, invalid producer identity, or
/// observations that are not bounded sorted objects with kinds.
pub fn parse_template(bytes: &[u8]) -> Result<SemanticEvidenceTemplate, Error> {
    let document: SemanticEvidenceTemplate = parse_document(bytes)?;
    validate_producer("$.producer", &document.producer)?;
    validate_observations("$.observations", &document.observations)?;
    Ok(document)
}

fn parse_document<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > SEMANTIC_EVIDENCE_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    de::deserialize_json(bytes)
}

/// Binds a template to one candidate, retaining the envelope and its canonical bytes.
///
/// # Errors
///
/// Fails when the template violates the same bounds [`envelope`] enforces or the resulting
/// envelope exceeds the byte ceiling.
pub fn bind_template<O: Serialize>(
    template: &SemanticEvidenceTemplate<O>,
    candidate_identity_digest: Digest,
) -> Result<(SemanticEvidenceEnvelope<&O>, Vec<u8>), Error> {
    envelope(SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest,
            source_report_payload_digest: Nullable::Null,
        },
        producer: template.producer.clone(),
        complete: template.complete,
        observations: template.observations.iter().collect(),
    })
}

/// Writes the canonical bytes of one candidate-independent semantic evidence template.
/// Observation order is canonicalized before the bytes are returned.
///
/// # Errors
///
/// Fails when producer metadata or an observation violates the same bounds [`parse_template`]
/// enforces, when observations repeat, or when the resulting template exceeds the byte ceiling.
pub fn template<O: Serialize>(input: SemanticEvidenceTemplate<O>) -> Result<Vec<u8>, Error> {
    validate_producer("$.producer", &input.producer)?;
    let observations = ordered_observations("$.observations", input.observations.iter().collect())?;
    canonical_bytes(&SemanticEvidenceTemplate {
        schema: input.schema,
        producer: input.producer,
        complete: input.complete,
        observations: observations.into(),
    })
}

/// Returns one digest-bound envelope together with its canonical bytes.
/// Observation order is canonicalized, so traversal order cannot change its identity.
///
/// # Errors
///
/// Fails when producer metadata or an observation violates the same bounds [`parse`] enforces,
/// when observations repeat, or when the resulting envelope exceeds the byte ceiling.
pub fn envelope<O: Serialize>(
    mut evidence: SemanticEvidence<O>,
) -> Result<(SemanticEvidenceEnvelope<O>, Vec<u8>), Error> {
    validate_producer("$.payload.producer", &evidence.producer)?;
    evidence.observations = ordered_observations("$.payload.observations", evidence.observations)?;
    let payload_digest = payload_digest(&evidence)?;
    let document = SemanticEvidenceEnvelope {
        schema: EnvelopeSchema::Current,
        payload: evidence,
        payload_digest,
    };
    let bytes = canonical_bytes(&document)?;
    Ok((document, bytes))
}

fn validate_producer(path: &str, producer: &SemanticProducer) -> Result<(), Error> {
    producer_version_valid(&producer.version)
        .then_some(())
        .ok_or_else(|| Error::new(&format!("{path}.version"), ErrorKind::InvalidValue))
}

#[must_use]
pub fn producer_version_valid(version: &str) -> bool {
    let Some((&first, tail)) = version.as_bytes().split_first() else {
        return false;
    };
    version.len() <= PRODUCER_VERSION_BYTES
        && first.is_ascii_alphanumeric()
        && tail
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
}

fn validate_observations(path: &str, observations: &[serde_json::Value]) -> Result<(), Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut previous: Option<Vec<u8>> = None;
    for (index, observation) in observations.iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let current = observation_key(&item_path, observation)?;
        match previous.as_deref().map(|value| value.cmp(&current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct ObservationHeader {
    kind: ArtifactId,
}

#[expect(
    clippy::zero_sized_map_values,
    reason = "Serde structs also accept arrays; observations must be objects"
)]
fn observation_key<O: Serialize>(path: &str, observation: &O) -> Result<Vec<u8>, Error> {
    let encoded = serde_json_canonicalizer::to_vec(observation)
        .map_err(|_defect| Error::new(path, ErrorKind::InvalidValue))?;
    serde_json::from_slice::<std::collections::BTreeMap<String, serde::de::IgnoredAny>>(&encoded)
        .map_err(|_defect| Error::new(path, ErrorKind::WrongType))?;
    let ObservationHeader { kind: _kind } =
        de::deserialize_json(&encoded).map_err(|mut defect| {
            defect.path = defect.path.replacen('$', path, 1);
            defect
        })?;
    Ok(encoded)
}

fn ordered_observations<O: Serialize>(path: &str, observations: Vec<O>) -> Result<Vec<O>, Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut keyed = Vec::with_capacity(observations.len());
    for (index, observation) in observations.into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let encoded = observation_key(&item_path, &observation)?;
        json::parse(&encoded).map_err(|defect| Error::new(&item_path, ErrorKind::Json(defect)))?;
        keyed.push((encoded, observation));
    }
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    if keyed
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.0 == right.0))
    {
        return fail(path, ErrorKind::DuplicateMember);
    }
    Ok(keyed.into_iter().map(|(_, value)| value).collect())
}

fn payload_digest<O: Serialize>(payload: &SemanticEvidence<O>) -> Result<Digest, Error> {
    hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(payload, &mut writer)
    })
    .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn canonical_bytes<T: Serialize>(document: &T) -> Result<Vec<u8>, Error> {
    let canonical = serde_json_canonicalizer::to_vec(document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > SEMANTIC_EVIDENCE_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    Ok(canonical)
}
