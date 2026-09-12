use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest as _;

use super::FileLedgerError;

const VERSION: u8 = 1;
const LENGTH_BYTES: usize = 8;
const DIGEST_BYTES: usize = 32;

#[derive(Clone, Copy)]
pub(crate) struct FrameFormat {
    magic: &'static [u8],
    domain: &'static str,
    maximum: u64,
}

pub(crate) const fn define(
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

pub(crate) fn encode<T: Serialize>(
    format: FrameFormat,
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), FileLedgerError>,
) -> Result<Vec<u8>, FileLedgerError> {
    validate(value)?;
    let payload = serde_json::to_vec(value).map_err(|_defect| FileLedgerError::Corrupt)?;
    let payload_length =
        u64::try_from(payload.len()).map_err(|_defect| FileLedgerError::Corrupt)?;
    let frame_length = format
        .magic
        .len()
        .checked_add(1 + LENGTH_BYTES + DIGEST_BYTES)
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(frame_length).map_err(|_defect| FileLedgerError::Corrupt)? > format.maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(format.magic);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(
        &sha2::Sha256::new_with_prefix(format.domain)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize(),
    );
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(crate) fn decode<T: DeserializeOwned + Serialize>(
    format: FrameFormat,
    frame: &[u8],
    validate: impl FnOnce(&T) -> Result<(), FileLedgerError>,
) -> Result<T, FileLedgerError> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > format.maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let (version, header) = frame
        .strip_prefix(format.magic)
        .and_then(<[u8]>::split_first)
        .ok_or(FileLedgerError::Corrupt)?;
    if *version != VERSION {
        return Err(FileLedgerError::Corrupt);
    }
    let (length, header) = header
        .split_first_chunk::<LENGTH_BYTES>()
        .ok_or(FileLedgerError::Corrupt)?;
    let (expected_digest, payload) = header
        .split_first_chunk::<DIGEST_BYTES>()
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(payload.len()).ok() != Some(u64::from_be_bytes(*length))
        || sha2::Sha256::new_with_prefix(format.domain)
            .chain_update([0_u8])
            .chain_update(payload)
            .finalize()
            .as_slice()
            != expected_digest
    {
        return Err(FileLedgerError::Corrupt);
    }
    let value = serde_json::from_slice(payload).map_err(|_defect| FileLedgerError::Corrupt)?;
    validate(&value)?;
    if serde_json::to_vec(&value).map_err(|_defect| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(value)
}
