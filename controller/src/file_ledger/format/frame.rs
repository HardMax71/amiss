use amiss_wire::digest::hb;

use super::record::Record;
use crate::file_ledger::FileLedgerError;

const RECORD_DOMAIN: &str = "amiss/controller-file-record-v1";
const FRAME_MAGIC: &[u8] = b"AMISS-DELIVERY-RECORD";
const FRAME_VERSION: u8 = 1;
const DIGEST_BYTES: usize = 32;

pub(in crate::file_ledger) const MAX_RECORD_BYTES: u64 = 131_072;

pub(in crate::file_ledger) fn encode(record: &Record) -> Result<Vec<u8>, FileLedgerError> {
    record.validate()?;
    let payload = serde_json::to_vec(record).map_err(|_| FileLedgerError::Corrupt)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| FileLedgerError::Corrupt)?;
    let digest_length = u64::try_from(DIGEST_BYTES).map_err(|_| FileLedgerError::Corrupt)?;
    let frame_length = u64::try_from(FRAME_MAGIC.len())
        .ok()
        .and_then(|length| length.checked_add(1))
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(digest_length))
        .and_then(|length| length.checked_add(payload_length))
        .ok_or(FileLedgerError::Corrupt)?;
    if frame_length > MAX_RECORD_BYTES {
        return Err(FileLedgerError::Corrupt);
    }
    let capacity = usize::try_from(frame_length).map_err(|_| FileLedgerError::Corrupt)?;
    let mut frame = Vec::with_capacity(capacity);
    frame.extend_from_slice(FRAME_MAGIC);
    frame.push(FRAME_VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(RECORD_DOMAIN, &payload).as_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(in crate::file_ledger) fn decode(frame: &[u8]) -> Result<Record, FileLedgerError> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > MAX_RECORD_BYTES {
        return Err(FileLedgerError::Corrupt);
    }
    let mut reader = FrameReader::new(frame);
    if reader.take(FRAME_MAGIC.len())? != FRAME_MAGIC || reader.byte()? != FRAME_VERSION {
        return Err(FileLedgerError::Corrupt);
    }
    let payload_length = reader.u64()?;
    let expected_digest = reader.take(DIGEST_BYTES)?;
    let payload = reader.rest();
    if u64::try_from(payload.len()).ok() != Some(payload_length)
        || hb(RECORD_DOMAIN, payload).as_bytes().as_slice() != expected_digest
    {
        return Err(FileLedgerError::Corrupt);
    }
    let record: Record = serde_json::from_slice(payload).map_err(|_| FileLedgerError::Corrupt)?;
    record.validate()?;
    if serde_json::to_vec(&record).map_err(|_| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(record)
}

struct FrameReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> FrameReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], FileLedgerError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(FileLedgerError::Corrupt)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(FileLedgerError::Corrupt)?;
        self.position = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, FileLedgerError> {
        let [byte] = self.take(1)? else {
            return Err(FileLedgerError::Corrupt);
        };
        Ok(*byte)
    }

    fn u64(&mut self) -> Result<u64, FileLedgerError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| FileLedgerError::Corrupt)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn rest(&self) -> &'a [u8] {
        self.bytes.get(self.position..).unwrap_or_default()
    }
}
