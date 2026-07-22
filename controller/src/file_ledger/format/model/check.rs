use amiss_wire::controls::valid_required_status_name;
use amiss_wire::digest::Digest;
use serde::{Deserialize, Serialize};

use crate::{CheckBinding, file_ledger::FileLedgerError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct StoredCheck {
    plan_digest: String,
    required_status_name: String,
    execution_constraint_digest: String,
}

pub(in crate::file_ledger) fn store(check: &CheckBinding) -> StoredCheck {
    StoredCheck {
        plan_digest: check.plan_digest.to_string(),
        required_status_name: check.required_status_name.clone(),
        execution_constraint_digest: check.execution_constraint_digest.to_string(),
    }
}

pub(in crate::file_ledger) fn materialize(
    stored: &StoredCheck,
) -> Result<CheckBinding, FileLedgerError> {
    let plan_digest = Digest::from_wire(&stored.plan_digest).ok_or(FileLedgerError::Corrupt)?;
    let execution_constraint_digest =
        Digest::from_wire(&stored.execution_constraint_digest).ok_or(FileLedgerError::Corrupt)?;
    valid_required_status_name(&stored.required_status_name)
        .then(|| CheckBinding {
            plan_digest,
            required_status_name: stored.required_status_name.clone(),
            execution_constraint_digest,
        })
        .ok_or(FileLedgerError::Corrupt)
}
