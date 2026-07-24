use amiss_wire::digest::hb;

use crate::file_ledger::FileLedgerError;

const DIGEST_BYTES: usize = 32;

pub(super) fn encode(
    magic: &[u8],
    version: u8,
    domain: &str,
    payload: &[u8],
    maximum: u64,
) -> Result<Vec<u8>, FileLedgerError> {
    let payload_length = u64::try_from(payload.len()).map_err(|_| FileLedgerError::Corrupt)?;
    let frame_length = magic
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(frame_length).map_err(|_| FileLedgerError::Corrupt)? > maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(magic);
    frame.push(version);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(domain, payload).as_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

pub(super) fn decode<'a>(
    frame: &'a [u8],
    magic: &[u8],
    version: u8,
    domain: &str,
    maximum: u64,
) -> Result<&'a [u8], FileLedgerError> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let header_length = magic
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .ok_or(FileLedgerError::Corrupt)?;
    let header = frame.get(..header_length).ok_or(FileLedgerError::Corrupt)?;
    let payload = frame.get(header_length..).ok_or(FileLedgerError::Corrupt)?;
    let magic_end = magic.len();
    if header.get(..magic_end) != Some(magic) || header.get(magic_end) != Some(&version) {
        return Err(FileLedgerError::Corrupt);
    }
    let length_start = magic_end.checked_add(1).ok_or(FileLedgerError::Corrupt)?;
    let length_end = length_start
        .checked_add(8)
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
        || hb(domain, payload).as_bytes().as_slice() != expected_digest
    {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(payload)
}
