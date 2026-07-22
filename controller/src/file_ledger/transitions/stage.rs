use crate::{ControllerEvaluationId, DeliveryLease, Publication, StageOutcome, StagedPublication};

use crate::file_ledger::format::{Record, State, StoredPublication};
use crate::file_ledger::store::Row;
use crate::file_ledger::{FileLedger, FileLedgerError};

impl FileLedger {
    pub(super) fn stage_running(
        &self,
        row: &Row,
        lease: &DeliveryLease,
        publication: &Publication,
        mut record: Record,
        evaluation_id: ControllerEvaluationId,
    ) -> Result<StageOutcome, FileLedgerError> {
        let State::Running {
            owner,
            fence,
            expires_at_unix_millis,
        } = record.state.clone()
        else {
            return Err(FileLedgerError::Corrupt);
        };
        if owner != self.owner
            || lease.evaluation_id != evaluation_id
            || lease.fence.get() != fence
            || lease.expires_at_unix_millis != expires_at_unix_millis
        {
            return Ok(StageOutcome::Lost);
        }
        let now = self.now(Some(&record))?;
        if now >= expires_at_unix_millis {
            return Ok(StageOutcome::Lost);
        }
        let stored = StoredPublication::new(publication)?;
        row.save_report(publication.report.as_deref(), stored.report())?;
        record.advance(now)?;
        record.state = State::Staged {
            fence,
            publication: Box::new(stored),
        };
        row.save(&record)?;
        Ok(StageOutcome::Staged(StagedPublication {
            evaluation_id,
            fence: lease.fence,
            publication: Box::new(publication.clone()),
        }))
    }
}
