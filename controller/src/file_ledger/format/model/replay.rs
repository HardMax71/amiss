use serde::{Deserialize, Serialize};

use crate::ingress::ReplayKeep;

use super::MaterializeResult;
use crate::file_ledger::FileLedgerError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "retention", rename_all = "kebab-case", deny_unknown_fields)]
pub(in crate::file_ledger::format) enum StoredReplayKeep {
    Permanent,
    KeepThrough { unix_millis: i64 },
}

impl StoredReplayKeep {
    pub(in crate::file_ledger::format) const fn new(replay: ReplayKeep) -> Self {
        match replay {
            ReplayKeep::Permanent => Self::Permanent,
            ReplayKeep::KeepThrough { unix_millis, .. } => Self::KeepThrough { unix_millis },
        }
    }

    pub(in crate::file_ledger::format) fn validate(&self) -> MaterializeResult<()> {
        match self {
            Self::Permanent => Ok(()),
            Self::KeepThrough { unix_millis } if *unix_millis >= 0 => Ok(()),
            Self::KeepThrough { .. } => Err(FileLedgerError::Corrupt),
        }
    }

    pub(in crate::file_ledger::format) const fn expired_at(&self, now: i64) -> bool {
        match self {
            Self::Permanent => false,
            Self::KeepThrough { unix_millis } => now > *unix_millis,
        }
    }
}
