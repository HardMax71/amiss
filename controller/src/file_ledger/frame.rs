use amiss_wire::digest::hb;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::FileLedgerError;

const VERSION: u8 = 1;
const LENGTH_BYTES: usize = 8;
const DIGEST_BYTES: usize = 32;

#[derive(Clone, Copy)]
pub(super) struct FrameFormat {
    magic: &'static [u8],
    domain: &'static str,
    maximum: u64,
}

pub(super) const fn define(
    magic: &'static [u8],
    domain: &'static str,
    maximum: u64,
) -> FrameFormat {
    FrameFormat {
        magic,
        domain,
        maximum,
    }
}

pub(super) fn encode<T: Serialize>(
    format: FrameFormat,
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), FileLedgerError>,
) -> Result<Vec<u8>, FileLedgerError> {
    validate(value)?;
    let payload = serde_json::to_vec(value).map_err(|_| FileLedgerError::Corrupt)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| FileLedgerError::Corrupt)?;
    let frame_length = format
        .magic
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(LENGTH_BYTES))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(frame_length).map_err(|_| FileLedgerError::Corrupt)? > format.maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(format.magic);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(format.domain, &payload).as_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(super) fn decode<T: DeserializeOwned + Serialize>(
    format: FrameFormat,
    frame: &[u8],
    validate: impl FnOnce(&T) -> Result<(), FileLedgerError>,
) -> Result<T, FileLedgerError> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > format.maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let header_length = format
        .magic
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(LENGTH_BYTES))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .ok_or(FileLedgerError::Corrupt)?;
    let header = frame.get(..header_length).ok_or(FileLedgerError::Corrupt)?;
    let payload = frame.get(header_length..).ok_or(FileLedgerError::Corrupt)?;
    let magic_end = format.magic.len();
    if header.get(..magic_end) != Some(format.magic) || header.get(magic_end) != Some(&VERSION) {
        return Err(FileLedgerError::Corrupt);
    }
    let length_start = magic_end.checked_add(1).ok_or(FileLedgerError::Corrupt)?;
    let length_end = length_start
        .checked_add(LENGTH_BYTES)
        .ok_or(FileLedgerError::Corrupt)?;
    let payload_length = u64::from_be_bytes(
        header
            .get(length_start..length_end)
            .ok_or(FileLedgerError::Corrupt)?
            .try_into()
            .map_err(|_| FileLedgerError::Corrupt)?,
    );
    let digest_end = length_end
        .checked_add(DIGEST_BYTES)
        .ok_or(FileLedgerError::Corrupt)?;
    let expected_digest = header
        .get(length_end..digest_end)
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(payload.len()).ok() != Some(payload_length)
        || hb(format.domain, payload).as_bytes().as_slice() != expected_digest
    {
        return Err(FileLedgerError::Corrupt);
    }
    let value = serde_json::from_slice(payload).map_err(|_| FileLedgerError::Corrupt)?;
    validate(&value)?;
    if serde_json::to_vec(&value).map_err(|_| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(value)
}
