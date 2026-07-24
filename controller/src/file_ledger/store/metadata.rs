use serde::{Deserialize, Serialize};

use super::frame;
use crate::file_ledger::{FileLedgerConfig, FileLedgerError};

const METADATA_SCHEMA: &str = "amiss/controller-file-root-v2";
const LEGACY_METADATA_SCHEMA: &str = "amiss/controller-file-root-v1";
const METADATA_DOMAIN: &str = "amiss/controller-file-root-frame-v1";
const FRAME_MAGIC: &[u8] = b"AMISS-DELIVERY-ROOT";
const FRAME_VERSION: u8 = 1;

pub(super) const MAX_METADATA_BYTES: u64 = 4_096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootMetadata {
    schema: String,
    lease_millis: i64,
    max_records: u64,
    max_signed_age_millis: i64,
    max_queue_age_millis: i64,
    clock_high_water_unix_millis: i64,
}

pub(super) enum StoredMetadata {
    Current(RootMetadata),
    Legacy(RootMetadata),
}

impl RootMetadata {
    pub(super) fn legacy(config: FileLedgerConfig, now: i64) -> Self {
        let replay_window = config.replay_window();
        Self {
            schema: LEGACY_METADATA_SCHEMA.to_owned(),
            lease_millis: config.lease_millis,
            max_records: config.max_records(),
            max_signed_age_millis: replay_window.max_signed_age_millis(),
            max_queue_age_millis: replay_window.max_queue_age_millis(),
            clock_high_water_unix_millis: now,
        }
    }

    pub(super) fn matches(&self, config: FileLedgerConfig) -> bool {
        let replay_window = config.replay_window();
        self.lease_millis == config.lease_millis
            && self.max_records == config.max_records()
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

    pub(super) fn upgrade(self) -> Self {
        Self {
            schema: METADATA_SCHEMA.to_owned(),
            ..self
        }
    }

    fn validate_values(&self) -> Result<(), FileLedgerError> {
        if self.lease_millis <= 0
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

impl StoredMetadata {
    pub(super) fn matches(&self, config: FileLedgerConfig) -> bool {
        match self {
            Self::Current(metadata) | Self::Legacy(metadata) => metadata.matches(config),
        }
    }

    pub(super) fn into_current(self) -> (RootMetadata, bool) {
        match self {
            Self::Current(metadata) => (metadata, false),
            Self::Legacy(metadata) => (metadata.upgrade(), true),
        }
    }
}

pub(super) fn encode(metadata: &RootMetadata) -> Result<Vec<u8>, FileLedgerError> {
    metadata.validate_values()?;
    if !matches!(
        metadata.schema.as_str(),
        METADATA_SCHEMA | LEGACY_METADATA_SCHEMA
    ) {
        return Err(FileLedgerError::Corrupt);
    }
    let payload = serde_json::to_vec(metadata).map_err(|_| FileLedgerError::Corrupt)?;
    frame::encode(
        FRAME_MAGIC,
        FRAME_VERSION,
        METADATA_DOMAIN,
        &payload,
        MAX_METADATA_BYTES,
    )
}

pub(super) fn decode(bytes: &[u8]) -> Result<StoredMetadata, FileLedgerError> {
    let payload = frame::decode(
        bytes,
        FRAME_MAGIC,
        FRAME_VERSION,
        METADATA_DOMAIN,
        MAX_METADATA_BYTES,
    )?;
    let metadata: RootMetadata =
        serde_json::from_slice(payload).map_err(|_| FileLedgerError::Corrupt)?;
    metadata.validate_values()?;
    if serde_json::to_vec(&metadata).map_err(|_| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    match metadata.schema.as_str() {
        METADATA_SCHEMA => Ok(StoredMetadata::Current(metadata)),
        LEGACY_METADATA_SCHEMA => Ok(StoredMetadata::Legacy(metadata)),
        _ => Err(FileLedgerError::Corrupt),
    }
}
