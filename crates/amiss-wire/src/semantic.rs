use std::cmp::Ordering;
use std::sync::Arc;

use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value, canonical, canonical_length};
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

struct Producer {
    kind: ArtifactId,
    identity: ArtifactId,
    version: String,
    context_digest: Digest,
    input_digest: Digest,
}

/// Parses one complete, digest-bound semantic evidence envelope.
///
/// # Errors
///
/// Fails on an oversized stream, strict-JSON or shape defects, a payload digest mismatch,
/// invalid producer identity, or observations that are not bounded sorted objects with kinds.
pub fn parse(bytes: &[u8]) -> Result<SemanticEvidenceEnvelope, Error> {
    let mut envelope = parse_document(bytes)?;
    envelope.required("schema", |path, value| {
        de::const_str(path, value, ENVELOPE_SCHEMA)
    })?;
    let payload = envelope.take("payload")?;
    let payload_digest = envelope.required("payload_digest", de::digest)?;
    envelope.finish()?;
    if hj(PAYLOAD_SCHEMA, &payload) != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(SemanticEvidenceEnvelope {
        payload: decode_payload("$.payload", payload)?,
        payload_digest,
    })
}

/// Parses one candidate-independent semantic evidence template.
///
/// # Errors
///
/// Fails on an oversized stream, strict-JSON or shape defects, invalid producer identity, or
/// observations that are not bounded sorted objects with kinds.
pub fn parse_template(bytes: &[u8]) -> Result<SemanticEvidenceTemplate, Error> {
    let mut template = parse_document(bytes)?;
    template.required("schema", |path, value| {
        de::const_str(path, value, TEMPLATE_SCHEMA)
    })?;
    let producer = decode_producer(&mut template)?;
    let complete = template.required("complete", de::boolean)?;
    let observations_path = template.field("observations");
    let observations = de::array(&observations_path, template.take("observations")?)?;
    template.finish()?;
    validate_observations(&observations_path, &observations)?;
    Ok(SemanticEvidenceTemplate {
        producer_kind: producer.kind,
        producer_identity: producer.identity,
        producer_version: producer.version,
        context_digest: producer.context_digest,
        input_digest: producer.input_digest,
        complete,
        observations: Arc::from(observations),
    })
}

fn parse_document(bytes: &[u8]) -> Result<Obj, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > SEMANTIC_EVIDENCE_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let value = json::parse(bytes).map_err(|error| Error::new("$", ErrorKind::Json(error)))?;
    Obj::new("$", value)
}

/// Binds a candidate-independent template to one exact scanner candidate.
///
/// # Errors
///
/// Fails when the template violates the same bounds [`envelope`] enforces or the resulting
/// envelope exceeds the byte ceiling.
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

/// Builds the unique candidate-independent value for one semantic evidence template.
/// Observation order is canonicalized before the value is emitted.
///
/// # Errors
///
/// Fails when producer metadata or an observation violates the same bounds [`parse_template`]
/// enforces, when observations repeat, or when the resulting template exceeds the byte ceiling.
pub fn template(input: SemanticEvidenceTemplate) -> Result<Value, Error> {
    if !producer_version_valid(&input.producer_version) {
        return fail("$.producer.version", ErrorKind::InvalidValue);
    }
    let observations =
        ordered_observations("$.observations", input.observations.as_ref().to_vec())?;
    let producer = Producer {
        kind: input.producer_kind,
        identity: input.producer_identity,
        version: input.producer_version,
        context_digest: input.context_digest,
        input_digest: input.input_digest,
    };
    let mut members = vec![("schema", text(TEMPLATE_SCHEMA))];
    members.extend(semantic_body(&producer, input.complete, observations));
    let value = object(members);
    if canonical_length(&value) > SEMANTIC_EVIDENCE_BYTES {
        fail("$", ErrorKind::LimitExceeded)
    } else {
        Ok(value)
    }
}

/// Builds the unique digest-bound value for one semantic evidence payload.
/// Observation order is canonicalized, so traversal order cannot change its identity.
///
/// # Errors
///
/// Fails when producer metadata or an observation violates the same bounds [`parse`] enforces,
/// when observations repeat, or when the resulting envelope exceeds the byte ceiling.
pub fn envelope(mut evidence: SemanticEvidence) -> Result<Value, Error> {
    if !producer_version_valid(&evidence.producer_version) {
        return fail("$.payload.producer.version", ErrorKind::InvalidValue);
    }
    evidence.observations = ordered_observations("$.payload.observations", evidence.observations)?;
    let payload = payload_value(evidence);
    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    let value = object(vec![
        ("schema", text(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", text(&payload_digest.to_string())),
    ]);
    if canonical_length(&value) > SEMANTIC_EVIDENCE_BYTES {
        fail("$", ErrorKind::LimitExceeded)
    } else {
        Ok(value)
    }
}

fn decode_payload(path: &str, value: Value) -> Result<SemanticEvidence, Error> {
    let mut payload = Obj::new(path, value)?;
    payload.required("schema", |path, value| {
        de::const_str(path, value, PAYLOAD_SCHEMA)
    })?;
    let subject_path = payload.field("subject");
    let mut subject = Obj::new(&subject_path, payload.take("subject")?)?;
    let candidate_identity_digest = subject.required("candidate_identity_digest", de::digest)?;
    let source_path = subject.field("source_report_payload_digest");
    let source_report_payload_digest = de::nullable(subject.take("source_report_payload_digest")?)
        .map(|value| de::digest(&source_path, value))
        .transpose()?;
    subject.finish()?;
    let producer = decode_producer(&mut payload)?;
    let complete = payload.required("complete", de::boolean)?;
    let observations_path = payload.field("observations");
    let observations = de::array(&observations_path, payload.take("observations")?)?;
    payload.finish()?;
    validate_observations(&observations_path, &observations)?;
    Ok(SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest,
        producer_kind: producer.kind,
        producer_identity: producer.identity,
        producer_version: producer.version,
        context_digest: producer.context_digest,
        input_digest: producer.input_digest,
        complete,
        observations,
    })
}

fn decode_producer(parent: &mut Obj) -> Result<Producer, Error> {
    let producer_path = parent.field("producer");
    let mut producer = Obj::new(&producer_path, parent.take("producer")?)?;
    let decoded = Producer {
        kind: producer.required("kind", decode_id)?,
        identity: producer.required("identity", decode_id)?,
        version: producer.required("version", decode_open_identity)?,
        context_digest: producer.required("context_digest", de::digest)?,
        input_digest: producer.required("input_digest", de::digest)?,
    };
    producer.finish()?;
    Ok(decoded)
}

fn decode_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn decode_open_identity(path: &str, value: Value) -> Result<String, Error> {
    let version = de::string(path, value)?;
    if producer_version_valid(&version) {
        Ok(version)
    } else {
        fail(path, ErrorKind::InvalidValue)
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
        let item_path = format!("{path}[{index}]");
        validate_observation(&item_path, observation)?;
        let current = canonical(observation);
        match previous.as_deref().map(|value| value.cmp(&current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}

fn validate_observation(path: &str, observation: &Value) -> Result<(), Error> {
    let Value::Object(_) = observation else {
        return fail(path, ErrorKind::WrongType);
    };
    let kind_path = format!("{path}.kind");
    observation
        .text("kind")
        .and_then(|kind| ArtifactId::new(kind.to_owned()))
        .ok_or_else(|| Error::new(&kind_path, ErrorKind::InvalidValue))?;
    Ok(())
}

fn ordered_observations(path: &str, observations: Vec<Value>) -> Result<Vec<Value>, Error> {
    if observations.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut keyed = Vec::with_capacity(observations.len());
    for (index, observation) in observations.into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let encoded = canonical(&observation);
        let observation = json::parse(&encoded)
            .map_err(|error| Error::new(&item_path, ErrorKind::Json(error)))?;
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

fn payload_value(evidence: SemanticEvidence) -> Value {
    let SemanticEvidence {
        candidate_identity_digest,
        source_report_payload_digest,
        producer_kind,
        producer_identity,
        producer_version,
        context_digest,
        input_digest,
        complete,
        observations,
    } = evidence;
    let producer = Producer {
        kind: producer_kind,
        identity: producer_identity,
        version: producer_version,
        context_digest,
        input_digest,
    };
    let mut members = vec![
        ("schema", text(PAYLOAD_SCHEMA)),
        (
            "subject",
            object(vec![
                (
                    "candidate_identity_digest",
                    text(&candidate_identity_digest.to_string()),
                ),
                (
                    "source_report_payload_digest",
                    source_report_payload_digest
                        .map_or(Value::Null, |digest| text(&digest.to_string())),
                ),
            ]),
        ),
    ];
    members.extend(semantic_body(&producer, complete, observations));
    object(members)
}

fn semantic_body(
    producer: &Producer,
    complete: bool,
    observations: Vec<Value>,
) -> Vec<(&'static str, Value)> {
    vec![
        (
            "producer",
            object(vec![
                ("kind", text(producer.kind.as_str())),
                ("identity", text(producer.identity.as_str())),
                ("version", text(&producer.version)),
                ("context_digest", text(&producer.context_digest.to_string())),
                ("input_digest", text(&producer.input_digest.to_string())),
            ]),
        ),
        ("complete", Value::Bool(complete)),
        (
            "observations",
            Value::Array(observations.into_boxed_slice()),
        ),
    ]
}
