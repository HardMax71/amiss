use serde::{Deserialize, Serialize};

use super::{frame, validate_key};
use crate::file_ledger::FileLedgerError;

const CAPACITY_SCHEMA: &str = "amiss/controller-file-capacity-v1";
const CAPACITY_DOMAIN: &str = "amiss/controller-file-capacity-frame-v1";
const FRAME_MAGIC: &[u8] = b"AMISS-DELIVERY-CAPACITY";
const FRAME_VERSION: u8 = 1;

pub(super) const MAX_CAPACITY_BYTES: u64 = 4_096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capacity {
    schema: String,
    pub(super) max_records: u64,
    pub(super) records: u64,
    pub(super) pending_key: Option<String>,
    pub(super) cleanup_pending: bool,
}

pub(super) fn ready(max_records: u64, records: u64) -> Result<Capacity, FileLedgerError> {
    let capacity = Capacity {
        schema: CAPACITY_SCHEMA.to_owned(),
        max_records,
        records,
        pending_key: None,
        cleanup_pending: false,
    };
    validate(&capacity)?;
    Ok(capacity)
}

pub(super) fn reserve(capacity: Capacity, key: &str) -> Result<Capacity, FileLedgerError> {
    let Capacity {
        schema,
        max_records,
        records,
        pending_key,
        cleanup_pending,
    } = capacity;
    if pending_key.is_some() || cleanup_pending {
        return Err(FileLedgerError::Corrupt);
    }
    if records >= max_records {
        return Err(FileLedgerError::Full);
    }
    validate_key(key)?;
    Ok(Capacity {
        schema,
        max_records,
        records: records.checked_add(1).ok_or(FileLedgerError::Corrupt)?,
        pending_key: Some(key.to_owned()),
        cleanup_pending,
    })
}

pub(super) fn begin_cleanup(capacity: Capacity) -> Result<Capacity, FileLedgerError> {
    let Capacity {
        schema,
        max_records,
        records,
        pending_key,
        cleanup_pending,
    } = capacity;
    if pending_key.is_some() || cleanup_pending || records == 0 {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(Capacity {
        schema,
        max_records,
        records,
        pending_key,
        cleanup_pending: true,
    })
}

pub(super) fn settle(
    capacity: Capacity,
    pending_file_is_present: bool,
) -> Result<Capacity, FileLedgerError> {
    let Capacity {
        schema,
        max_records,
        records,
        pending_key,
        cleanup_pending,
    } = capacity;
    if pending_key.is_none() || cleanup_pending {
        return Err(FileLedgerError::Corrupt);
    }
    let records = if pending_file_is_present {
        records
    } else {
        records.checked_sub(1).ok_or(FileLedgerError::Corrupt)?
    };
    let settled = Capacity {
        schema,
        max_records,
        records,
        pending_key: None,
        cleanup_pending,
    };
    validate(&settled)?;
    Ok(settled)
}

pub(super) fn finish_cleanup(
    capacity: Capacity,
    records: u64,
) -> Result<Capacity, FileLedgerError> {
    let Capacity {
        schema,
        max_records,
        records: previous,
        pending_key,
        cleanup_pending,
    } = capacity;
    if !cleanup_pending || pending_key.is_some() || records > previous {
        return Err(FileLedgerError::Corrupt);
    }
    let capacity = Capacity {
        schema,
        max_records,
        records,
        pending_key,
        cleanup_pending: false,
    };
    validate(&capacity)?;
    Ok(capacity)
}

pub(super) fn reconcile_cleanup(
    capacity: Capacity,
    records: u64,
) -> Result<Capacity, FileLedgerError> {
    if capacity.cleanup_pending {
        finish_cleanup(capacity, records)
    } else if capacity.records == records {
        Ok(capacity)
    } else {
        Err(FileLedgerError::Corrupt)
    }
}

fn validate(capacity: &Capacity) -> Result<(), FileLedgerError> {
    if capacity.schema != CAPACITY_SCHEMA
        || capacity.max_records == 0
        || capacity.records > capacity.max_records
        || capacity.pending_key.is_some() && capacity.records == 0
        || capacity.cleanup_pending && (capacity.pending_key.is_some() || capacity.records == 0)
        || capacity
            .pending_key
            .as_deref()
            .is_some_and(|key| validate_key(key).is_err())
    {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(())
}

pub(super) fn encode(capacity: &Capacity) -> Result<Vec<u8>, FileLedgerError> {
    validate(capacity)?;
    let payload = serde_json::to_vec(capacity).map_err(|_| FileLedgerError::Corrupt)?;
    frame::encode(
        FRAME_MAGIC,
        FRAME_VERSION,
        CAPACITY_DOMAIN,
        &payload,
        MAX_CAPACITY_BYTES,
    )
}

pub(super) fn decode(bytes: &[u8]) -> Result<Capacity, FileLedgerError> {
    let payload = frame::decode(
        bytes,
        FRAME_MAGIC,
        FRAME_VERSION,
        CAPACITY_DOMAIN,
        MAX_CAPACITY_BYTES,
    )?;
    let capacity: Capacity =
        serde_json::from_slice(payload).map_err(|_| FileLedgerError::Corrupt)?;
    validate(&capacity)?;
    if serde_json::to_vec(&capacity).map_err(|_| FileLedgerError::Corrupt)? != payload {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(capacity)
}
