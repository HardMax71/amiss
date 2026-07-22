use crate::{
    AuthenticatedDelivery, ControllerEvaluationId, DeliveryClaim, DeliveryLease, DeliveryLedger,
    LeaseCompletion, LeaseFence, LeaseRenewal, Publication, StageOutcome, StagedPublication,
};

use super::format::{self, State, StoredPublication};
use super::store::Row;
use super::{FileLedger, FileLedgerError};

impl DeliveryLedger for FileLedger {
    type Error = FileLedgerError;

    fn claim(&mut self, delivery: &AuthenticatedDelivery) -> Result<DeliveryClaim, Self::Error> {
        let row = self.row(delivery)?;
        let Some(record) = row.load()? else {
            return self.claim_new(&row, delivery);
        };
        if !record.matches(delivery) {
            return Ok(DeliveryClaim::BindingConflict);
        }
        let evaluation_id = record.evaluation_id()?;
        match record.state.clone() {
            State::Running { .. } => self.claim_running(&row, record, evaluation_id),
            State::Staged { fence, publication } => Ok(DeliveryClaim::Publish(staged(
                &row,
                evaluation_id,
                fence,
                &publication,
            )?)),
            State::Done { .. } => Ok(DeliveryClaim::Duplicate { evaluation_id }),
        }
    }

    fn renew(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error> {
        let row = self.row(delivery)?;
        let Some(mut record) = row.load()? else {
            return Ok(LeaseRenewal::Lost);
        };
        if !record.matches(delivery) {
            return Ok(LeaseRenewal::Lost);
        }
        let State::Running {
            owner,
            fence,
            expires_at_unix_millis,
        } = record.state
        else {
            return Ok(LeaseRenewal::Lost);
        };
        let evaluation_id = record.evaluation_id()?;
        if owner != self.owner
            || lease.evaluation_id != evaluation_id
            || lease.fence.get() != fence
            || lease.expires_at_unix_millis != expires_at_unix_millis
        {
            return Ok(LeaseRenewal::Lost);
        }
        let now = self.now(Some(&record))?;
        if now >= expires_at_unix_millis {
            return Ok(LeaseRenewal::Lost);
        }
        let renewed_deadline = expires_at_unix_millis.max(self.deadline(now)?);
        record.advance(now)?;
        record.state = State::Running {
            owner,
            fence,
            expires_at_unix_millis: renewed_deadline,
        };
        row.save(&record)?;
        Ok(LeaseRenewal::Renewed(make_lease(
            evaluation_id,
            fence,
            renewed_deadline,
        )?))
    }

    fn stage(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
        publication: &Publication,
    ) -> Result<StageOutcome, Self::Error> {
        let row = self.row(delivery)?;
        let Some(record) = row.load()? else {
            return Ok(StageOutcome::Lost);
        };
        let evaluation_id = record.evaluation_id()?;
        if !record.matches(delivery) || !publication_matches(delivery, &evaluation_id, publication)
        {
            return Ok(StageOutcome::Lost);
        }
        match record.state.clone() {
            State::Staged {
                fence,
                publication: stored,
            } => restage(&row, lease, publication, evaluation_id, fence, &stored),
            State::Done { .. } => Ok(StageOutcome::Lost),
            State::Running { .. } => {
                self.stage_running(&row, lease, publication, record, evaluation_id)
            }
        }
    }

    fn complete(
        &mut self,
        delivery: &AuthenticatedDelivery,
        staged_publication: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error> {
        let row = self.row(delivery)?;
        let Some(mut record) = row.load()? else {
            return Ok(LeaseCompletion::Lost);
        };
        let evaluation_id = record.evaluation_id()?;
        if !record.matches(delivery) || staged_publication.evaluation_id != evaluation_id {
            return Ok(LeaseCompletion::Lost);
        }
        let requested = match StoredPublication::new(&staged_publication.publication) {
            Ok(publication) => publication,
            Err(FileLedgerError::ReportTooLarge) => return Ok(LeaseCompletion::Lost),
            Err(error) => return Err(error),
        };
        let requested_digest =
            format::staged_digest(&evaluation_id, staged_publication.fence.get(), &requested)?;
        match record.state.clone() {
            State::Done {
                fence,
                staged_digest,
            } if fence == staged_publication.fence.get() && staged_digest == requested_digest => {
                Ok(LeaseCompletion::Completed)
            }
            State::Done { .. } | State::Running { .. } => Ok(LeaseCompletion::Lost),
            State::Staged { fence, publication } => {
                if fence != staged_publication.fence.get()
                    || format::staged_digest(&evaluation_id, fence, &publication)?
                        != requested_digest
                    || staged(&row, evaluation_id, fence, &publication)? != *staged_publication
                {
                    return Ok(LeaseCompletion::Lost);
                }
                record.advance(record.last_seen_unix_millis)?;
                record.state = State::Done {
                    fence,
                    staged_digest: requested_digest,
                };
                row.save(&record)?;
                Ok(LeaseCompletion::Completed)
            }
        }
    }
}

fn restage(
    row: &Row,
    lease: &DeliveryLease,
    publication: &Publication,
    evaluation_id: ControllerEvaluationId,
    fence: u64,
    stored: &StoredPublication,
) -> Result<StageOutcome, FileLedgerError> {
    let existing = staged(row, evaluation_id, fence, stored)?;
    let requested = StagedPublication {
        evaluation_id: lease.evaluation_id.clone(),
        fence: lease.fence,
        publication: Box::new(publication.clone()),
    };
    if existing == requested {
        Ok(StageOutcome::Staged(existing))
    } else {
        Ok(StageOutcome::Lost)
    }
}

fn make_lease(
    evaluation_id: ControllerEvaluationId,
    fence: u64,
    expires_at_unix_millis: i64,
) -> Result<DeliveryLease, FileLedgerError> {
    Ok(DeliveryLease {
        evaluation_id,
        fence: LeaseFence::new(fence).ok_or(FileLedgerError::Corrupt)?,
        expires_at_unix_millis,
    })
}

fn staged(
    row: &Row,
    evaluation_id: ControllerEvaluationId,
    fence: u64,
    stored: &StoredPublication,
) -> Result<StagedPublication, FileLedgerError> {
    let report = row.load_report(stored.report())?;
    Ok(StagedPublication {
        evaluation_id,
        fence: LeaseFence::new(fence).ok_or(FileLedgerError::Corrupt)?,
        publication: Box::new(stored.materialize(report)?),
    })
}

fn publication_matches(
    delivery: &AuthenticatedDelivery,
    evaluation_id: &ControllerEvaluationId,
    publication: &Publication,
) -> bool {
    publication.evaluation_id == *evaluation_id
        && publication.provider_run == delivery.provider_run
        && publication.run.change == delivery.change
        && publication.run.object_format == delivery.provider_run.object_format
        && publication.run.commits.candidate == delivery.provider_run.candidate_commit
}
mod claim;
mod stage;
