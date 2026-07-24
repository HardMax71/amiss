use serde::{Deserialize, Serialize};

use crate::file_ledger::{FileLedgerConfig, FileLedgerError, frame};

const METADATA_SCHEMA: &str = "amiss/controller-file-root-v2";
const LEGACY_METADATA_SCHEMA: &str = "amiss/controller-file-root-v1";

pub(super) const MAX_METADATA_BYTES: u64 = 4_096;

const METADATA_FRAME: frame::FrameFormat = frame::define(
    b"AMISS-DELIVERY-ROOT",
    "amiss/controller-file-root-frame-v1",
    MAX_METADATA_BYTES,
);

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
    frame::encode(METADATA_FRAME, metadata, validate)
}

pub(super) fn decode(bytes: &[u8]) -> Result<StoredMetadata, FileLedgerError> {
    let metadata = frame::decode(METADATA_FRAME, bytes, validate)?;
    match metadata.schema.as_str() {
        METADATA_SCHEMA => Ok(StoredMetadata::Current(metadata)),
        LEGACY_METADATA_SCHEMA => Ok(StoredMetadata::Legacy(metadata)),
        _ => Err(FileLedgerError::Corrupt),
    }
}

fn validate(metadata: &RootMetadata) -> Result<(), FileLedgerError> {
    metadata.validate_values()?;
    matches!(
        metadata.schema.as_str(),
        METADATA_SCHEMA | LEGACY_METADATA_SCHEMA
    )
    .then_some(())
    .ok_or(FileLedgerError::Corrupt)
}
