use std::collections::BTreeMap;

use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj};
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

/// Parses one bounded normalized record-set input.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, unknown fields, invalid identities or digests,
/// and records that are not bounded, control-free, sorted, and unique by key.
pub fn parse_input(bytes: &[u8]) -> Result<Input, Error> {
    let mut input = super::parse_document(bytes)?;
    input.required("schema", |path, value| {
        de::const_str(path, value, INPUT_SCHEMA)
    })?;
    let producer_identity = input.required("producer_identity", super::decode_id)?;
    let context_digest = input.required("context_digest", de::digest)?;
    let input_digest = input.required("input_digest", de::digest)?;
    let complete = input.required("complete", de::boolean)?;
    let name = input.required("name", super::decode_id)?;
    let records = input.required("records", decode_records)?;
    input.finish()?;
    Ok(Input {
        producer_identity,
        context_digest,
        input_digest,
        complete,
        set: Observation { name, records },
    })
}

/// Produces the canonical semantic template value for one validated record-set input.
///
/// # Errors
///
/// Fails only if the fixed producer contract or the encoded template exceeds the semantic
/// evidence bounds.
pub fn template(input: Input) -> Result<Value, Error> {
    let producer_kind = ArtifactId::new(PRODUCER_KIND.to_owned())
        .ok_or_else(|| Error::new("$.producer.kind", ErrorKind::InvalidValue))?;
    let observation = observation_value(input.set);
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

/// Decodes the closed record-set observation grammar shared by producers and the scanner.
///
/// # Errors
///
/// Fails on an incorrect kind, unknown fields, an invalid set name, or invalid record rows.
pub fn decode_observation(path: &str, value: Value) -> Result<Observation, Error> {
    let mut observation = Obj::new(path, value)?;
    observation.required("kind", |path, value| {
        de::const_str(path, value, PRODUCER_KIND)
    })?;
    let name = observation.required("name", super::decode_id)?;
    let records = observation.required("records", decode_records)?;
    observation.finish()?;
    Ok(Observation { name, records })
}

fn decode_records(path: &str, value: Value) -> Result<BTreeMap<String, String>, Error> {
    de::sorted_map(path, value, SEMANTIC_OBSERVATIONS_LIMIT, |path, value| {
        let mut row = Obj::new(path, value)?;
        let key = row.required("key", |path, value| {
            de::bounded_text(path, value, super::RECORD_KEY_BYTES)
        })?;
        let value = row.required("value", |path, value| {
            de::bounded_text(path, value, super::RECORD_VALUE_BYTES)
        })?;
        row.finish()?;
        Ok((key, value))
    })
}

fn observation_value(observation: Observation) -> Value {
    let records = observation
        .records
        .into_iter()
        .map(|(key, value)| object(vec![("key", text(&key)), ("value", text(&value))]))
        .collect();
    object(vec![
        ("kind", text(PRODUCER_KIND)),
        ("name", text(observation.name.as_str())),
        ("records", Value::array(records)),
    ])
}
