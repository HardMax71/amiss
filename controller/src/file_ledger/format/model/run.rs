use amiss_wire::model::ObjectFormat;
use serde::{Deserialize, Serialize};

use crate::{OidPair, RunIdentity, RunRefs};

use super::delivery::StoredChange;
use super::{MaterializeResult, checked};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredRun {
    pub(in crate::file_ledger::format) change: StoredChange,
    pub(in crate::file_ledger::format) refs: RunRefs,
    pub(in crate::file_ledger::format) object_format: ObjectFormat,
    pub(in crate::file_ledger::format) commits: OidPair,
    pub(in crate::file_ledger::format) trees: OidPair,
}

impl StoredRun {
    pub(in crate::file_ledger::format) fn materialize(&self) -> MaterializeResult<RunIdentity> {
        checked(RunIdentity::new(
            self.change.materialize()?,
            self.refs.clone(),
            self.object_format,
            self.commits.clone(),
            self.trees.clone(),
        ))
    }
}
