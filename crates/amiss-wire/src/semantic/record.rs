use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::de::{self, Error, ErrorKind};
use crate::digest::Digest;
use crate::model::ArtifactId;

use super::{
    SEMANTIC_OBSERVATIONS_LIMIT, SemanticEvidenceTemplate, SemanticProducer, TemplateSchema,
};

pub const INPUT_SCHEMA: &str = "amiss/record-set-input";
pub const PRODUCER_KIND: &str = "record-set";
pub const PRODUCER_VERSION: &str = "1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema: InputSchema,
    pub producer_identity: ArtifactId,
    pub context_digest: Digest,
    pub input_digest: Digest,
    pub complete: bool,
    pub name: ArtifactId,
    pub records: Vec<Record>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSchema {
    #[serde(rename = "amiss/record-set-input")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub kind: ObservationKind,
    pub name: ArtifactId,
    pub records: Vec<Record>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationKind {
    #[serde(rename = "record-set")]
    Current,
}

/// Parses one bounded normalized record-set input.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, unknown fields, invalid identities or digests,
/// and records that are not bounded, control-free, sorted, and unique by key.
pub fn parse_input(bytes: &[u8]) -> Result<Input, Error> {
    let input: Input = super::parse_document(bytes)?;
    validate_records("$.records", &input.records)?;
    Ok(input)
}

/// Produces canonical semantic template bytes for one validated record-set input.
///
/// # Errors
///
/// Fails when a directly constructed input violates the reader's record laws, the fixed producer
/// contract is invalid, or the encoded template exceeds the semantic evidence bounds.
pub fn template(input: Input) -> Result<Vec<u8>, Error> {
    validate_records("$.records", &input.records)?;
    let producer_kind = ArtifactId::new(PRODUCER_KIND.to_owned())
        .ok_or_else(|| Error::new("$.producer.kind", ErrorKind::InvalidValue))?;
    let observation = serde_json::to_value(Observation {
        kind: ObservationKind::Current,
        name: input.name,
        records: input.records,
    })
    .map_err(|_defect| Error::new("$.observations[0]", ErrorKind::InvalidValue))?;
    super::template(SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: producer_kind,
            identity: input.producer_identity,
            version: PRODUCER_VERSION.to_owned(),
            context_digest: input.context_digest,
            input_digest: input.input_digest,
        },
        complete: input.complete,
        observations: vec![observation].into(),
    })
}

/// Decodes the closed record-set observation grammar shared by producers and the scanner.
///
/// # Errors
///
/// Fails on an incorrect kind, unknown fields, an invalid set name, or invalid record rows.
pub fn decode_observation(path: &str, value: serde_json::Value) -> Result<Observation, Error> {
    let observation: Observation = de::deserialize_value(path, value)?;
    validate_records(&format!("{path}.records"), &observation.records)?;
    Ok(observation)
}

fn validate_records(path: &str, records: &[Record]) -> Result<(), Error> {
    (records.len() <= SEMANTIC_OBSERVATIONS_LIMIT)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    let mut previous: Option<&str> = None;
    for (index, record) in records.iter().enumerate() {
        for (field, value, limit) in [
            ("key", record.key.as_str(), super::RECORD_KEY_BYTES),
            ("value", record.value.as_str(), super::RECORD_VALUE_BYTES),
        ] {
            (!value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control))
                .then_some(())
                .ok_or_else(|| {
                    Error::new(&format!("{path}[{index}].{field}"), ErrorKind::InvalidValue)
                })?;
        }
        if let Some(previous) = previous {
            match previous.cmp(&record.key) {
                Ordering::Less => {}
                Ordering::Equal => return de::fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return de::fail(path, ErrorKind::UnsortedSet),
            }
        }
        previous = Some(&record.key);
    }
    Ok(())
}
