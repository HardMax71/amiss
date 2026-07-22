use amiss_wire::digest::hb;
use serde::{Deserialize, Serialize};

use crate::file_ledger::{FileLedgerConfig, FileLedgerError};

const METADATA_SCHEMA: &str = "amiss/controller-file-root-v1";
const METADATA_DOMAIN: &str = "amiss/controller-file-root-frame-v1";
const FRAME_MAGIC: &[u8] = b"AMISS-DELIVERY-ROOT";
const FRAME_VERSION: u8 = 1;
const DIGEST_BYTES: usize = 32;

pub(super) const MAX_METADATA_BYTES: u64 = 4_096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootMetadata {
    schema: String,
    max_records: u64,
    max_signed_age_millis: i64,
    max_queue_age_millis: i64,
    clock_high_water_unix_millis: i64,
}

impl RootMetadata {
    pub(super) fn new(config: FileLedgerConfig, now: i64) -> Self {
        let replay_window = config.replay_window();
        Self {
            schema: METADATA_SCHEMA.to_owned(),
            max_records: config.max_records(),
            max_signed_age_millis: replay_window.max_signed_age_millis(),
            max_queue_age_millis: replay_window.max_queue_age_millis(),
            clock_high_water_unix_millis: now,
        }
    }

    pub(super) fn matches(&self, config: FileLedgerConfig) -> bool {
        let replay_window = config.replay_window();
        self.max_records == config.max_records()
            && self.max_signed_age_millis == replay_window.max_signed_age_millis()
            && self.max_queue_age_millis == replay_window.max_queue_age_millis()
    }

    pub(super) const fn clock_high_water_unix_millis(&self) -> i64 {
        self.clock_high_water_unix_millis
    }

    pub(super) fn advance_clock(&mut self, now: i64) -> Result<i64, FileLedgerError> {
        if now < 0 {
            return Err(FileLedgerError::Clock);
        }
        self.clock_high_water_unix_millis = self.clock_high_water_unix_millis.max(now);
        Ok(self.clock_high_water_unix_millis)
    }

    fn validate(&self) -> Result<(), FileLedgerError> {
        if self.schema != METADATA_SCHEMA
            || self.max_records == 0
            || self.max_signed_age_millis <= 0
            || self.max_queue_age_millis <= 0
            || self.clock_high_water_unix_millis < 0
        {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(())
    }
}

pub(super) fn encode(metadata: &RootMetadata) -> Result<Vec<u8>, FileLedgerError> {
    metadata.validate()?;
    let payload = serde_json::to_vec(metadata).map_err(|_| FileLedgerError::Corrupt)?;
    let payload_length = u64::try_from(payload.len()).map_err(|_| FileLedgerError::Corrupt)?;
    let frame_length = FRAME_MAGIC
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(FileLedgerError::Corrupt)?;
    if u64::try_from(frame_length).map_err(|_| FileLedgerError::Corrupt)? > MAX_METADATA_BYTES {
        return Err(FileLedgerError::Corrupt);
    }
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(FRAME_MAGIC);
    frame.push(FRAME_VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(METADATA_DOMAIN, &payload).as_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(super) fn decode(frame: &[u8]) -> Result<RootMetadata, FileLedgerError> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > MAX_METADATA_BYTES {
        return Err(FileLedgerError::Corrupt);
    }
    let header_length = FRAME_MAGIC
        .len()
        .checked_add(1)
        .and_then(|length| length.checked_add(8))
        .and_then(|length| length.checked_add(DIGEST_BYTES))
        .ok_or(FileLedgerError::Corrupt)?;
    let header = frame.get(..header_length).ok_or(FileLedgerError::Corrupt)?;
    let payload = frame.get(header_length..).ok_or(FileLedgerError::Corrupt)?;
    let magic_end = FRAME_MAGIC.len();
    if header.get(..magic_end) != Some(FRAME_MAGIC) || header.get(magic_end) != Some(&FRAME_VERSION)
    {
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
        || hb(METADATA_DOMAIN, payload).as_bytes().as_slice() != expected_digest
    {
        return Err(FileLedgerError::Corrupt);
    }
    let metadata: RootMetadata =
        serde_json::from_slice(payload).map_err(|_| FileLedgerError::Corrupt)?;
    metadata.validate()?;
    if serde_json::to_vec(&metadata).map_err(|_| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(metadata)
}
