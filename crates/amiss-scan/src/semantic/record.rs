use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use amiss_wire::de::{Error, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;

use super::RecordSet;

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
    let decoded = amiss_wire::semantic::record::decode_observation(&observation_path, observation)?;
    match sets.entry(decoded.name) {
        Entry::Vacant(slot) => {
            slot.insert(RecordSet {
                complete,
                records: decoded.records,
            });
            Ok(())
        }
        Entry::Occupied(_) => fail(
            &format!("{observation_path}.name"),
            ErrorKind::DuplicateMember,
        ),
    }
}
