use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::ArtifactId;

use super::{SEMANTIC_OBSERVATIONS_LIMIT, SemanticEvidenceTemplate};

pub const INPUT_SCHEMA: &str = "amiss/record-set-input";
pub const PRODUCER_KIND: &str = "record-set";
pub const PRODUCER_VERSION: &str = "1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub producer_identity: ArtifactId,
    pub context_digest: Digest,
    pub input_digest: Digest,
    pub complete: bool,
    pub set: Observation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub name: ArtifactId,
    pub records: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputDocument {
    schema: String,
    producer_identity: ArtifactId,
    context_digest: Digest,
    input_digest: Digest,
    complete: bool,
    name: ArtifactId,
    records: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationDocument {
    kind: String,
    name: ArtifactId,
    records: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    key: String,
    value: String,
}

/// Parses one bounded normalized record-set input.
///
/// # Errors
///
/// The document has a shape defect or its record rows exceed the sorted, bounded contract.
pub fn parse_input(bytes: &[u8]) -> Result<Input, Error> {
    let input: InputDocument = super::parse_document(bytes)?;
    if input.schema != INPUT_SCHEMA {
        return fail("$.schema", ErrorKind::InvalidValue);
    }
    Ok(Input {
        producer_identity: input.producer_identity,
        context_digest: input.context_digest,
        input_digest: input.input_digest,
        complete: input.complete,
        set: Observation {
            name: input.name,
            records: checked_records("$.records", input.records)?,
        },
    })
}

/// Produces the canonical semantic template for a record set.
///
/// # Errors
///
/// The records or the encoded template exceed the semantic evidence contract.
pub fn template(input: Input) -> Result<Value, Error> {
    let producer_kind = ArtifactId::new(PRODUCER_KIND.to_owned())
        .ok_or_else(|| Error::new("$.producer.kind", ErrorKind::InvalidValue))?;
    let records: Vec<_> = input
        .set
        .records
        .into_iter()
        .map(|(key, value)| Record { key, value })
        .collect();
    validate_records("$.observations[0].records", &records)?;
    let observation = codec::to_value(&ObservationDocument {
        kind: PRODUCER_KIND.to_owned(),
        name: input.set.name,
        records,
    })?;
    super::template(SemanticEvidenceTemplate {
        producer_kind,
        producer_identity: input.producer_identity,
        producer_version: PRODUCER_VERSION.to_owned(),
        context_digest: input.context_digest,
        input_digest: input.input_digest,
        complete: input.complete,
        observations: vec![observation].into(),
    })
}

/// Decodes the closed record-set observation grammar shared by producer and scanner.
///
/// # Errors
///
/// The observation has a shape defect or invalid, repeated, or unsorted records.
pub fn decode_observation(path: &str, value: &Value) -> Result<Observation, Error> {
    let observation: ObservationDocument = codec::from_value(path, value)?;
    if observation.kind != PRODUCER_KIND {
        return fail(&format!("{path}.kind"), ErrorKind::InvalidValue);
    }
    Ok(Observation {
        name: observation.name,
        records: checked_records(&format!("{path}.records"), observation.records)?,
    })
}

fn checked_records(path: &str, records: Vec<Record>) -> Result<BTreeMap<String, String>, Error> {
    validate_records(path, &records)?;
    Ok(records
        .into_iter()
        .map(|record| (record.key, record.value))
        .collect())
}

fn validate_records(path: &str, records: &[Record]) -> Result<(), Error> {
    if records.len() > SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        for (name, value, limit) in [
            ("key", &record.key, super::RECORD_KEY_BYTES),
            ("value", &record.value, super::RECORD_VALUE_BYTES),
        ] {
            if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
                return fail(&format!("{path}[{index}].{name}"), ErrorKind::InvalidValue);
            }
        }
    }
    for pair in records.windows(2) {
        if let [left, right] = pair {
            match left.key.cmp(&right.key) {
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
                Ordering::Less => {}
            }
        }
    }
    Ok(())
}
