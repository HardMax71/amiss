mod tests;

use std::io::Read as _;

use serde::de::DeserializeOwned;

use crate::ProviderError;

/// Reads and decodes one JSON body without crossing its byte ceiling.
///
/// # Errors
///
/// Returns [`ProviderError::InvalidResponse`] for an oversized declaration or
/// body and malformed JSON, and [`ProviderError::Unavailable`] for a read
/// failure.
pub fn decode_bounded_json<T: DeserializeOwned>(
    reader: impl std::io::Read,
    declared_bytes: Option<u64>,
    maximum_bytes: usize,
) -> Result<(T, usize), ProviderError> {
    let maximum = u64::try_from(maximum_bytes).map_err(|_defect| ProviderError::InvalidResponse)?;
    if declared_bytes.is_some_and(|declared| declared > maximum) {
        return Err(ProviderError::InvalidResponse);
    }
    let limit = maximum
        .checked_add(1)
        .ok_or(ProviderError::InvalidResponse)?;
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_defect| ProviderError::Unavailable)?;
    if bytes.len() > maximum_bytes {
        return Err(ProviderError::InvalidResponse);
    }
    let length = bytes.len();
    let value = serde_json::from_slice(&bytes).map_err(|_defect| ProviderError::InvalidResponse)?;
    Ok((value, length))
}
