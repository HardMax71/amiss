mod tests;

use amiss_wire::model::{ObjectFormat, Oid};
use serde::{Deserialize, Serialize};

use crate::file_ledger::FileLedgerError;
use crate::{OidPair, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunRefs};

use super::MaterializeResult;
use super::delivery::StoredChange;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredProviderRun {
    pub(in crate::file_ledger::format) run_id: String,
    pub(in crate::file_ledger::format) attempt: ProviderRunAttempt,
    pub(in crate::file_ledger::format) object_format: ObjectFormat,
    pub(in crate::file_ledger::format) candidate_commit: Oid,
}

impl StoredProviderRun {
    pub(in crate::file_ledger::format) fn materialize(
        &self,
    ) -> MaterializeResult<ProviderRunIdentity> {
        ProviderRunId::new(self.run_id.clone())
            .and_then(|run_id| {
                ProviderRunIdentity::new(
                    run_id,
                    self.attempt,
                    self.object_format,
                    self.candidate_commit.clone(),
                )
            })
            .ok_or(FileLedgerError::Corrupt)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredRun {
    pub(in crate::file_ledger::format) change: StoredChange,
    pub(in crate::file_ledger::format) refs: RunRefs,
    pub(in crate::file_ledger::format) object_format: ObjectFormat,
    pub(in crate::file_ledger::format) commits: OidPair,
    pub(in crate::file_ledger::format) trees: OidPair,
}
