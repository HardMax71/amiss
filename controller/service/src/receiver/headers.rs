use axum::http::HeaderMap;

use crate::DeliveryHeader;

pub(crate) fn within_limits(headers: &HeaderMap, max_headers: u64, max_header_bytes: u64) -> bool {
    u64::try_from(headers.len()).is_ok_and(|count| count <= max_headers)
        && headers
            .iter()
            .try_fold(0_u64, |total, (name, value)| {
                let name_bytes = u64::try_from(name.as_str().len()).ok()?;
                let value_bytes = u64::try_from(value.as_bytes().len()).ok()?;
                total
                    .checked_add(name_bytes)
                    .and_then(|total| total.checked_add(value_bytes))
            })
            .is_some_and(|bytes| bytes <= max_header_bytes)
}

pub(crate) fn materialize(headers: &HeaderMap) -> Vec<DeliveryHeader> {
    let mut normalized = headers
        .iter()
        .map(|(name, value)| DeliveryHeader {
            name: name.as_str().to_owned(),
            value: value.as_bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    normalized.sort_unstable_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.value.cmp(&right.value))
    });
    normalized
}
