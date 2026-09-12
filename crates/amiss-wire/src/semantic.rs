use std::cmp::Ordering;
use std::sync::Arc;

use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::{Value, canonical_length};
use crate::model::ArtifactId;

pub mod record;

pub const ENVELOPE_SCHEMA: &str = "amiss/semantic-evidence-envelope";
pub const PAYLOAD_SCHEMA: &str = "amiss/semantic-evidence-payload";
pub const TEMPLATE_SCHEMA: &str = "amiss/semantic-evidence-template";
pub const SEMANTIC_EVIDENCE_BYTES: u64 = 16_777_216;
pub const SEMANTIC_OBSERVATIONS_LIMIT: usize = 100_000;
pub const PRODUCER_VERSION_BYTES: usize = 128;
pub const RECORD_KEY_BYTES: usize = 4_096;
pub const RECORD_VALUE_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvidence {
    pub candidate_identity_digest: Digest,
    pub source_report_payload_digest: Option<Digest>,
    pub producer_kind: ArtifactId,
    pub producer_identity: ArtifactId,
    pub producer_version: String,
    pub context_digest: Digest,
    pub input_digest: Digest,
    pub complete: bool,
    pub observations: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvidenceEnvelope {
    pub payload: SemanticEvidence,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvidenceTemplate {
    pub producer_kind: ArtifactId,
    pub producer_identity: ArtifactId,
    pub producer_version: String,
    pub context_digest: Digest,
    pub input_digest: Digest,
    pub complete: bool,
    pub observations: Arc<[Value]>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Producer {
    kind: ArtifactId,
    identity: ArtifactId,
    version: String,
    context_digest: Digest,
    input_digest: Digest,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    candidate_identity_digest: Digest,
    #[serde(deserialize_with = "codec::nullable")]
    source_report_payload_digest: Option<Digest>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadDocument {
    schema: String,
    subject: Subject,
    producer: Producer,
    complete: bool,
    observations: Vec<Value>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateDocument {
    schema: String,
    producer: Producer,
    complete: bool,
    observations: Vec<Value>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeDocument {
    schema: String,
    payload: Value,
    payload_digest: Digest,
}

/// Parses one bounded, complete, digest-bound semantic evidence envelope.
///
/// # Errors
///
/// The document has a syntax, shape, identity, ordering, limit, or digest defect.
pub fn parse(bytes: &[u8]) -> Result<SemanticEvidenceEnvelope, Error> {
    decode(&parse_document(bytes)?)
}

/// Checks an embedded envelope directly, retaining unknown observations in its digest.
///
/// # Errors
///
/// As [`parse`].
pub fn decode(value: &Value) -> Result<SemanticEvidenceEnvelope, Error> {
    codec::check_value("$", value)?;
    if canonical_length(value) > SEMANTIC_EVIDENCE_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let envelope: EnvelopeDocument = codec::borrow_value("$", value)?;
    if envelope.schema != ENVELOPE_SCHEMA {
        return fail("$.schema", ErrorKind::InvalidValue);
    }
    if codec::digest(PAYLOAD_SCHEMA, &envelope.payload)? != envelope.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    let payload: PayloadDocument = codec::owned_value("$.payload", envelope.payload)?;
    if payload.schema != PAYLOAD_SCHEMA {
        return fail("$.payload.schema", ErrorKind::InvalidValue);
    }
    validate_producer("$.payload.producer", &payload.producer)?;
    validate_observations("$.payload.observations", &payload.observations)?;
    Ok(SemanticEvidenceEnvelope {
        payload: SemanticEvidence {
            candidate_identity_digest: payload.subject.candidate_identity_digest,
            source_report_payload_digest: payload.subject.source_report_payload_digest,
            producer_kind: payload.producer.kind,
            producer_identity: payload.producer.identity,
            producer_version: payload.producer.version,
            context_digest: payload.producer.context_digest,
            input_digest: payload.producer.input_digest,
            complete: payload.complete,
            observations: payload.observations,
        },
        payload_digest: envelope.payload_digest,
    })
}

/// Parses a candidate-independent template with bounded, sorted observations.
///
/// # Errors
///
/// The document has a syntax, shape, identity, ordering, or limit defect.
pub fn parse_template(bytes: &[u8]) -> Result<SemanticEvidenceTemplate, Error> {
    let document: TemplateDocument = parse_document(bytes)?;
    if document.schema != TEMPLATE_SCHEMA {
        return fail("$.schema", ErrorKind::InvalidValue);
    }
    validate_producer("$.producer", &document.producer)?;
    validate_observations("$.observations", &document.observations)?;
    Ok(SemanticEvidenceTemplate {
        producer_kind: document.producer.kind,
        producer_identity: document.producer.identity,
        producer_version: document.producer.version,
        context_digest: document.producer.context_digest,
        input_digest: document.producer.input_digest,
        complete: document.complete,
        observations: document.observations.into(),
    })
}

fn parse_document<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > SEMANTIC_EVIDENCE_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    codec::decode(bytes)
}

/// Binds a validated template to one exact candidate.
///
/// # Errors
///
/// The template or the resulting envelope exceeds the semantic evidence contract.
pub fn bind_template(
    template: &SemanticEvidenceTemplate,
    candidate_identity_digest: Digest,
) -> Result<Value, Error> {
    envelope(SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest: None,
        producer_kind: template.producer_kind.clone(),
        producer_identity: template.producer_identity.clone(),
        producer_version: template.producer_version.clone(),
        context_digest: template.context_digest,
        input_digest: template.input_digest,
        complete: template.complete,
        observations: template.observations.as_ref().to_vec(),
    })
}

/// Builds a canonical template, ordering observations independently of traversal order.
///
/// # Errors
///
/// Metadata, observations, or encoded size violate the semantic evidence contract.
pub fn template(input: SemanticEvidenceTemplate) -> Result<Value, Error> {
    let producer = Producer {
        kind: input.producer_kind,
        identity: input.producer_identity,
        version: input.producer_version,
        context_digest: input.context_digest,
        input_digest: input.input_digest,
    };
    validate_producer("$.producer", &producer)?;
    bounded_value(&TemplateDocument {
        schema: TEMPLATE_SCHEMA.to_owned(),
        producer,
        complete: input.complete,
        observations: ordered_observations("$.observations", input.observations.as_ref().to_vec())?,
    })
}

/// Builds a digest-bound envelope, ordering observations independently of traversal order.
///
/// # Errors
///
/// Metadata, observations, or encoded size violate the semantic evidence contract.
pub fn envelope(evidence: SemanticEvidence) -> Result<Value, Error> {
    let producer = Producer {
        kind: evidence.producer_kind,
        identity: evidence.producer_identity,
        version: evidence.producer_version,
        context_digest: evidence.context_digest,
        input_digest: evidence.input_digest,
    };
    validate_producer("$.payload.producer", &producer)?;
    let payload = codec::to_value(&PayloadDocument {
        schema: PAYLOAD_SCHEMA.to_owned(),
        subject: Subject {
            candidate_identity_digest: evidence.candidate_identity_digest,
            source_report_payload_digest: evidence.source_report_payload_digest,
        },
        producer,
        complete: evidence.complete,
        observations: ordered_observations("$.payload.observations", evidence.observations)?,
    })?;
    let payload_digest = codec::digest(PAYLOAD_SCHEMA, &payload)?;
    bounded_value(&EnvelopeDocument {
        schema: ENVELOPE_SCHEMA.to_owned(),
        payload,
        payload_digest,
    })
}

fn bounded_value<T: serde::Serialize>(document: &T) -> Result<Value, Error> {
    let value = codec::to_value(document)?;
    if canonical_length(&value) > SEMANTIC_EVIDENCE_BYTES {
        fail("$", ErrorKind::LimitExceeded)
    } else {
        Ok(value)
    }
}

fn validate_producer(path: &str, producer: &Producer) -> Result<(), Error> {
    if producer_version_valid(&producer.version) {
        Ok(())
    } else {
        fail(&format!("{path}.version"), ErrorKind::InvalidValue)
    }
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

fn validate_observations(path: &str, observations: &[Value]) -> Result<(), Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut previous: Option<Vec<u8>> = None;
    for (index, observation) in observations.iter().enumerate() {
        validate_observation(&format!("{path}[{index}]"), observation)?;
        let current = codec::canonical(observation)?;
        match previous.as_deref().map(|value| value.cmp(&current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}

fn validate_observation(path: &str, observation: &Value) -> Result<(), Error> {
    #[derive(serde::Deserialize)]
    struct ObservationKind {
        #[serde(rename = "kind")]
        _kind: ArtifactId,
    }
    codec::from_value::<ObservationKind>(path, observation).map(|_| ())
}

fn ordered_observations(path: &str, observations: Vec<Value>) -> Result<Vec<Value>, Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut keyed = Vec::with_capacity(observations.len());
    for (index, observation) in observations.into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let encoded = codec::canonical(&observation).map_err(|mut error| {
            error.path = format!(
                "{item_path}{}",
                error.path.strip_prefix('$').unwrap_or_default()
            );
            error
        })?;
        validate_observation(&item_path, &observation)?;
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
