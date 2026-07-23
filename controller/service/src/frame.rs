use std::mem::size_of;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::InboxError;
use crate::hash::digest;

const VERSION: u8 = 1;
const DIGEST_BYTES: usize = 32;

pub(crate) fn encode<T: Serialize>(
    magic: &[u8],
    domain: &str,
    value: &T,
) -> Result<Vec<u8>, InboxError> {
    let payload = serde_json::to_vec(value).map_err(|_| InboxError::Corrupt)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| InboxError::Corrupt)?;
    let header_bytes = header_bytes(magic)?;
    let capacity = header_bytes
        .checked_add(payload.len())
        .ok_or(InboxError::Corrupt)?;
    let mut frame = Vec::with_capacity(capacity);
    frame.extend_from_slice(magic);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(&digest(domain, &payload));
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(crate) fn decode<T: DeserializeOwned + Serialize>(
    magic: &[u8],
    domain: &str,
    frame: &[u8],
) -> Result<T, InboxError> {
    let version_offset = magic.len();
    let length_start = version_offset.checked_add(1).ok_or(InboxError::Corrupt)?;
    let length_end = length_start
        .checked_add(size_of::<u64>())
        .ok_or(InboxError::Corrupt)?;
    let digest_end = length_end
        .checked_add(DIGEST_BYTES)
        .ok_or(InboxError::Corrupt)?;
    if frame.get(..version_offset) != Some(magic) || frame.get(version_offset) != Some(&VERSION) {
        return Err(InboxError::Corrupt);
    }
    let payload_length = u64::from_be_bytes(
        frame
            .get(length_start..length_end)
            .ok_or(InboxError::Corrupt)?
            .try_into()
            .map_err(|_| InboxError::Corrupt)?,
    );
    let expected_digest = frame
        .get(length_end..digest_end)
        .ok_or(InboxError::Corrupt)?;
    let payload = frame.get(digest_end..).ok_or(InboxError::Corrupt)?;
    if u64::try_from(payload.len()).ok() != Some(payload_length)
        || digest(domain, payload).as_slice() != expected_digest
    {
        return Err(InboxError::Corrupt);
    }
    let value: T = serde_json::from_slice(payload).map_err(|_| InboxError::Corrupt)?;
    if serde_json::to_vec(&value).map_err(|_| InboxError::Corrupt)? != payload {
        return Err(InboxError::Corrupt);
    }
    Ok(value)
}

fn header_bytes(magic: &[u8]) -> Result<usize, InboxError> {
    magic
        .len()
        .checked_add(1)
        .and_then(|bytes| bytes.checked_add(size_of::<u64>()))
        .and_then(|bytes| bytes.checked_add(DIGEST_BYTES))
        .ok_or(InboxError::Corrupt)
}
