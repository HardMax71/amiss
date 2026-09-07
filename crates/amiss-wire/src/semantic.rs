use std::{borrow::Cow, cmp::Ordering, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hj_serde};
use crate::json;
use crate::model::ArtifactId;

pub mod observation;
pub mod record;

use observation::Observation;

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
pub struct SemanticEvidenceEnvelope<'a> {
    pub schema: EnvelopeSchema,
    pub payload: SemanticEvidence<'a>,
    pub payload_digest: Digest,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EnvelopeSchema {
    #[strum(serialize = "amiss/semantic-evidence-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidence<'a> {
    pub schema: PayloadSchema,
    pub subject: SemanticSubject,
    pub producer: SemanticProducer,
    pub complete: bool,
    pub observations: Vec<Cow<'a, Observation>>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PayloadSchema {
    #[strum(serialize = "amiss/semantic-evidence-payload")]
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
    pub kind: SemanticProducerKind,
    pub identity: ArtifactId,
    pub version: String,
    pub context_digest: Digest,
    pub input_digest: Digest,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
    strum::EnumIter,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SemanticProducerKind {
    SphinxInventorySet,
    SiteBuild,
    RecordSet,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticEvidenceTemplate<'a> {
    pub schema: TemplateSchema,
    pub producer: SemanticProducer,
    pub complete: bool,
    pub observations: Arc<[Cow<'a, Observation>]>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum TemplateSchema {
    #[strum(serialize = "amiss/semantic-evidence-template")]
    Current,
}

/// Parses one complete, digest-bound semantic evidence envelope.
///
/// # Errors
///
/// Fails on an oversized stream, strict-JSON or shape defects, a payload digest mismatch,
/// invalid producer identity, or observations outside the closed, bounded, sorted contract.
pub fn parse(bytes: &[u8]) -> Result<SemanticEvidenceEnvelope<'static>, Error> {
    let document: SemanticEvidenceEnvelope<'static> = parse_document(bytes)?;
    validate(&document)?;
    Ok(document)
}

/// Checks the digest and semantic laws of a decoded evidence envelope.
/// Readers enforce byte limits and strict JSON before decoding.
///
/// # Errors
///
/// Fails on a digest mismatch, invalid producer identity, or invalid observation set.
pub fn validate(document: &SemanticEvidenceEnvelope<'_>) -> Result<(), Error> {
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
/// observations outside the closed, bounded, sorted contract.
pub fn parse_template(bytes: &[u8]) -> Result<SemanticEvidenceTemplate<'static>, Error> {
    let document: SemanticEvidenceTemplate<'static> = parse_document(bytes)?;
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
pub fn bind_template<'a>(
    template: &'a SemanticEvidenceTemplate<'_>,
    candidate_identity_digest: Digest,
) -> Result<(SemanticEvidenceEnvelope<'a>, Vec<u8>), Error> {
    envelope(SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest,
            source_report_payload_digest: Nullable::Null,
        },
        producer: template.producer.clone(),
        complete: template.complete,
        observations: template
            .observations
            .iter()
            .map(|row| Cow::Borrowed(row.as_ref()))
            .collect(),
    })
}

/// Writes the canonical bytes of one candidate-independent semantic evidence template.
/// Observation order is canonicalized before the bytes are returned.
///
/// # Errors
///
/// Fails when producer metadata or an observation violates the same bounds [`parse_template`]
/// enforces, when observations repeat, or when the resulting template exceeds the byte ceiling.
pub fn template(input: SemanticEvidenceTemplate<'_>) -> Result<Vec<u8>, Error> {
    validate_producer("$.producer", &input.producer)?;
    let observations = ordered_observations(
        "$.observations",
        input
            .observations
            .iter()
            .map(|row| Cow::Borrowed(row.as_ref()))
            .collect(),
    )?;
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
pub fn envelope(
    mut evidence: SemanticEvidence<'_>,
) -> Result<(SemanticEvidenceEnvelope<'_>, Vec<u8>), Error> {
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

fn validate_observations(path: &str, observations: &[Cow<'_, Observation>]) -> Result<(), Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut previous: Option<Vec<u8>> = None;
    for (index, observation) in observations.iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let current = serde_json_canonicalizer::to_vec(observation)
            .map_err(|_defect| Error::new(&item_path, ErrorKind::InvalidValue))?;
        match previous.as_deref().map(|value| value.cmp(&current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}

fn ordered_observations<'a>(
    path: &str,
    observations: Vec<Cow<'a, Observation>>,
) -> Result<Vec<Cow<'a, Observation>>, Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut keyed = Vec::with_capacity(observations.len());
    for (index, observation) in observations.into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let encoded = serde_json_canonicalizer::to_vec(observation.as_ref())
            .map_err(|_defect| Error::new(&item_path, ErrorKind::InvalidValue))?;
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

fn payload_digest(payload: &SemanticEvidence<'_>) -> Result<Digest, Error> {
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
    Ok(canonical)
}
