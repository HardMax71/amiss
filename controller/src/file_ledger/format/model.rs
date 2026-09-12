mod conclusion;
mod delivery;
mod replay;
mod run;

use crate::file_ledger::FileLedgerError;

pub(super) use conclusion::StoredConclusion;
pub(super) use delivery::{StoredChange, StoredDelivery, StoredDeliveryKey};
pub(super) use replay::StoredReplayKeep;
pub(super) use run::StoredRun;

type MaterializeResult<T> = Result<T, FileLedgerError>;

fn checked<T>(value: Option<T>) -> MaterializeResult<T> {
    value.ok_or(FileLedgerError::Corrupt)
}
