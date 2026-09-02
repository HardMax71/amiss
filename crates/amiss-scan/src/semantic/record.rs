use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use amiss_wire::assessment::Nullable;
use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;

use super::RecordSet;

pub(super) fn insert_record_set(
    sets: &mut BTreeMap<ArtifactId, RecordSet>,
    path: &str,
    source_report_payload_digest: Nullable<Digest>,
    complete: bool,
    observations: Vec<serde_json::Value>,
) -> Result<(), Error> {
    if source_report_payload_digest != Nullable::Null {
        return fail(path, ErrorKind::Inconsistent);
    }
    let observations_path = format!("{path}.payload.observations");
    let [observation]: [serde_json::Value; 1] = observations
        .try_into()
        .map_err(|_values| Error::new(&observations_path, ErrorKind::Inconsistent))?;
    let observation_path = format!("{path}.payload.observations[0]");
    let decoded = amiss_wire::semantic::record::decode_observation(&observation_path, observation)?;
    match sets.entry(decoded.name) {
        Entry::Vacant(slot) => {
            slot.insert(RecordSet {
                complete,
                records: decoded
                    .records
                    .into_iter()
                    .map(|record| (record.key, record.value))
                    .collect(),
            });
            Ok(())
        }
        Entry::Occupied(_) => fail(
            &format!("{observation_path}.name"),
            ErrorKind::DuplicateMember,
        ),
    }
}
