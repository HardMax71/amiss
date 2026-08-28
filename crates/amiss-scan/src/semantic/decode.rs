use std::cmp::Ordering;

use amiss_wire::de::{self, Error, ErrorKind, Obj, fail};
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;

pub(super) const LABEL_BYTES: usize = 4_096;
pub(super) const DESTINATION_BYTES: usize = 16_384;

pub(super) fn repo_path(path: &str, value: Value) -> Result<amiss_wire::model::RepoPath, Error> {
    amiss_wire::model::RepoPath::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(super) fn sorted_set<T: Ord>(
    path: &str,
    value: Value,
    item_count: &mut usize,
    mut decode: impl FnMut(&str, Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    let values = de::array(path, value)?;
    *item_count = item_count
        .checked_add(values.len())
        .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    let mut items: Vec<T> = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let item = decode(&format!("{path}[{index}]"), value)?;
        match items.last().map(|previous| previous.cmp(&item)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => items.push(item),
        }
    }
    Ok(items)
}

pub(super) fn observation_row(path: &str, observation: Value, kind: &str) -> Result<Obj, Error> {
    let mut row = Obj::new(path, observation)?;
    row.required("kind", |path, value| de::const_str(path, value, kind))?;
    Ok(row)
}

pub(super) fn decode_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(super) fn bounded_text(
    path: &str,
    value: Value,
    limit: usize,
    valid: impl FnOnce(&str) -> bool,
) -> Result<String, Error> {
    let text = de::string(path, value)?;
    if text.len() <= limit && valid(&text) {
        Ok(text)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}
