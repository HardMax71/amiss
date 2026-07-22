use crate::{AcceptedDelivery, CheckBinding, ControllerEvaluationId, DeliveryClaim};

use super::make_lease;
use crate::file_ledger::format::{Record, State};
use crate::file_ledger::store::Row;
use crate::file_ledger::{FileLedger, FileLedgerError};

impl FileLedger {
    pub(super) fn claim_new(
        &self,
        row: &Row,
        delivery: &AcceptedDelivery,
        check: &CheckBinding,
    ) -> Result<DeliveryClaim, FileLedgerError> {
        let now = self.now(row, None)?;
        let evaluation_id = Self::new_evaluation_id(&delivery.delivery().identity)?;
        let expires_at_unix_millis = self.deadline(now)?;
        let record = Record::running(
            delivery,
            check,
            &evaluation_id,
            self.owner,
            now,
            expires_at_unix_millis,
        );
        row.save_new(&record)?;
        Ok(DeliveryClaim::Execute(make_lease(
            evaluation_id,
            check.clone(),
            1,
            expires_at_unix_millis,
        )?))
    }

    pub(super) fn claim_running(
        &self,
        row: &Row,
        mut record: Record,
        evaluation_id: ControllerEvaluationId,
        check: &CheckBinding,
    ) -> Result<DeliveryClaim, FileLedgerError> {
        let State::Running {
            owner,
            fence,
            expires_at_unix_millis,
        } = record.state.clone()
        else {
            return Err(FileLedgerError::Corrupt);
        };
        let now = self.now(row, Some(&record))?;
        if now < expires_at_unix_millis {
            if owner == self.owner {
                return Ok(DeliveryClaim::Execute(make_lease(
                    evaluation_id,
                    check.clone(),
                    fence,
                    expires_at_unix_millis,
                )?));
            }
            return Ok(DeliveryClaim::Busy {
                evaluation_id,
                retry_at_unix_millis: expires_at_unix_millis,
            });
        }
        let fence = fence.checked_add(1).ok_or(FileLedgerError::Corrupt)?;
        let expires_at_unix_millis = self.deadline(now)?;
        record.advance(now)?;
        record.state = State::Running {
            owner: self.owner,
            fence,
            expires_at_unix_millis,
        };
        row.save(&record)?;
        Ok(DeliveryClaim::Execute(make_lease(
            evaluation_id,
            check.clone(),
            fence,
            expires_at_unix_millis,
        )?))
    }
}
