use std::cmp::Ordering;

use amiss_wire::de::{Error, ErrorKind, fail};

pub(super) const LABEL_BYTES: usize = 4_096;
pub(super) const DESTINATION_BYTES: usize = 16_384;

pub(super) fn sorted_set<T: Ord>(
    path: &str,
    items: &[T],
    item_count: &mut usize,
) -> Result<(), Error> {
    *item_count = item_count
        .checked_add(items.len())
        .filter(|count| *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT)
        .ok_or_else(|| Error::new(path, ErrorKind::LimitExceeded))?;
    for pair in items.windows(2) {
        if let [left, right] = pair {
            match left.cmp(right) {
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
                Ordering::Less => {}
            }
        }
    }
    Ok(())
}

pub(super) fn bounded_text(
    path: &str,
    text: &str,
    limit: usize,
    valid: impl FnOnce(&str) -> bool,
) -> Result<(), Error> {
    if text.len() <= limit && valid(text) {
        Ok(())
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}
