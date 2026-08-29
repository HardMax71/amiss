use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use amiss_wire::de::{self, Error, ErrorKind, Obj, fail};
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;

use super::RecordSet;
use super::decode::{bounded_text, decode_id, observation_row};

const RECORD_SET: &str = "record-set";

pub(super) fn insert_record_set(
    sets: &mut BTreeMap<ArtifactId, RecordSet>,
    path: &str,
    source_report_payload_digest: Option<Digest>,
    complete: bool,
    observations: Vec<Value>,
) -> Result<(), Error> {
    if source_report_payload_digest.is_some() {
        return fail(path, ErrorKind::Inconsistent);
    }
    let observations_path = format!("{path}.payload.observations");
    let [observation]: [Value; 1] = observations
        .try_into()
        .map_err(|_values| Error::new(&observations_path, ErrorKind::Inconsistent))?;
    let observation_path = format!("{path}.payload.observations[0]");
    let mut set = observation_row(&observation_path, observation, RECORD_SET)?;
    let name = set.required("name", decode_id)?;
    let records = set.required("records", decode_records)?;
    set.finish()?;
    match sets.entry(name) {
        Entry::Vacant(slot) => {
            slot.insert(RecordSet { complete, records });
            Ok(())
        }
        Entry::Occupied(_) => fail(
            &format!("{observation_path}.name"),
            ErrorKind::DuplicateMember,
        ),
    }
}

fn decode_records(path: &str, value: Value) -> Result<BTreeMap<String, String>, Error> {
    let values = de::array(path, value)?;
    if values.len() > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut records: BTreeMap<String, String> = BTreeMap::new();
    for (index, value) in values.into_iter().enumerate() {
        let row_path = format!("{path}[{index}]");
        let mut row = Obj::new(&row_path, value)?;
        let key = row.required("key", |path, value| {
            bounded_text(
                path,
                value,
                amiss_wire::semantic::RECORD_KEY_BYTES,
                |text| !text.is_empty() && !text.chars().any(char::is_control),
            )
        })?;
        let value = row.required("value", |path, value| {
            bounded_text(
                path,
                value,
                amiss_wire::semantic::RECORD_VALUE_BYTES,
                |text| !text.is_empty() && !text.chars().any(char::is_control),
            )
        })?;
        row.finish()?;
        match records
            .last_key_value()
            .map(|(previous, _value)| previous.cmp(&key))
        {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => {
                records.insert(key, value);
            }
        }
    }
    Ok(records)
}
