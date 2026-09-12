use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, thiserror::Error)]
pub enum JsonInputError {
    #[error("the JSON input exceeds its byte ceiling")]
    LimitExceeded,
    #[error("the input is not strict JSON")]
    Json(#[from] crate::json::Error),
    #[error("the JSON input does not match its complete typed contract")]
    Shape(#[from] serde_json::Error),
    #[error("{0}")]
    Canonical(#[from] crate::digest::CanonicalJsonError),
}

/// Reads a bounded, strict JSON document without discarding or normalizing fields.
///
/// # Errors
///
/// Rejects oversized or malformed input, typed decoding failures, and any change
/// to the canonical input when the decoded document is serialized.
pub fn read_json<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    limit: u64,
) -> Result<T, JsonInputError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(JsonInputError::LimitExceeded);
    }
    crate::json::parse(bytes)?;
    let mut input = serde_json::Deserializer::from_slice(bytes);
    // The strict gate has already enforced the document depth ceiling.
    input.disable_recursion_limit();
    let document = T::deserialize(&mut input)?;
    input.end()?;
    crate::digest::verified_json_digest("amiss/typed-json-input", bytes, &document)?;
    Ok(document)
}
