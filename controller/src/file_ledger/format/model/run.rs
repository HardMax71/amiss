use amiss_wire::model::ObjectFormat;
use serde::{Deserialize, Serialize};

use crate::{OidPair, RunIdentity, RunRefs};

use super::delivery::StoredChange;
use super::{MaterializeResult, checked};
use crate::file_ledger::FileLedgerError;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredRun {
    pub(in crate::file_ledger::format) change: StoredChange,
    pub(in crate::file_ledger::format) refs: RunRefs,
    pub(in crate::file_ledger::format) object_format: ObjectFormat,
    pub(in crate::file_ledger::format) commits: OidPair,
    pub(in crate::file_ledger::format) trees: OidPair,
}

impl StoredRun {
    pub(in crate::file_ledger::format) fn materialize(self) -> MaterializeResult<RunIdentity> {
        checked(RunIdentity::new(
            self.change.materialize()?,
            self.refs,
            self.object_format,
            self.commits,
            self.trees,
        ))
    }

    pub(in crate::file_ledger::format) fn validate(&self) -> MaterializeResult<()> {
        (self.change.is_valid()
            && self.commits.well_formed(self.object_format)
            && self.trees.well_formed(self.object_format))
        .then_some(())
        .ok_or(FileLedgerError::Corrupt)
    }
}
