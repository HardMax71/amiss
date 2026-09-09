mod conclusion;
mod delivery;
mod replay;
mod run;

use crate::file_ledger::FileLedgerError;

pub(super) use conclusion::StoredConclusion;
pub(super) use delivery::{StoredChange, StoredDelivery, StoredDeliveryKey};
pub(super) use replay::StoredReplayKeep;
pub(super) use run::{StoredProviderRun, StoredRun};

type MaterializeResult<T> = Result<T, FileLedgerError>;
