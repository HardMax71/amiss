use std::{cmp::Ordering, collections::BTreeMap};

use serde::{Deserialize, Serialize};

use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj};
use crate::digest::Digest;
use crate::json::{self, Value};
use crate::model::ArtifactId;

use super::{SEMANTIC_OBSERVATIONS_LIMIT, SemanticEvidenceTemplate};

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
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > super::SEMANTIC_EVIDENCE_BYTES {
        return de::fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let input: Input = de::deserialize_json(bytes)?;
    validate_input(&input)?;
    Ok(input)
}

/// Produces the canonical semantic template value for one validated record-set input.
///
/// # Errors
///
/// Fails when a directly constructed input violates the reader's record laws, the fixed producer
/// contract is invalid, or the encoded template exceeds the semantic evidence bounds.
pub fn template(input: Input) -> Result<Value, Error> {
    validate_input(&input)?;
    let Input {
        schema: InputSchema::Current,
        producer_identity,
        context_digest,
        input_digest,
        complete,
        name,
        records,
    } = input;
    let producer_kind = ArtifactId::new(PRODUCER_KIND.to_owned())
        .ok_or_else(|| Error::new("$.producer.kind", ErrorKind::InvalidValue))?;
    let observation = object(vec![
        ("kind", text(PRODUCER_KIND)),
        ("name", text(name.as_str())),
        (
            "records",
            Value::array(
                records
                    .into_iter()
                    .map(|record| {
                        object(vec![
                            ("key", text(&record.key)),
                            ("value", text(&record.value)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    super::template(SemanticEvidenceTemplate {
        producer_kind,
        producer_identity,
        producer_version: PRODUCER_VERSION.to_owned(),
        context_digest,
        input_digest,
        complete,
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

fn validate_input(input: &Input) -> Result<(), Error> {
    (input.records.len() <= SEMANTIC_OBSERVATIONS_LIMIT)
        .then_some(())
        .ok_or_else(|| Error::new("$.records", ErrorKind::LimitExceeded))?;
    let mut previous: Option<&str> = None;
    for (index, record) in input.records.iter().enumerate() {
        for (field, value, limit) in [
            ("key", record.key.as_str(), super::RECORD_KEY_BYTES),
            ("value", record.value.as_str(), super::RECORD_VALUE_BYTES),
        ] {
            (!value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control))
                .then_some(())
                .ok_or_else(|| {
                    Error::new(
                        &format!("$.records[{index}].{field}"),
                        ErrorKind::InvalidValue,
                    )
                })?;
        }
        if let Some(previous) = previous {
            match previous.cmp(&record.key) {
                Ordering::Less => {}
                Ordering::Equal => return de::fail("$.records", ErrorKind::DuplicateMember),
                Ordering::Greater => return de::fail("$.records", ErrorKind::UnsortedSet),
            }
        }
        previous = Some(&record.key);
    }
    Ok(())
}
