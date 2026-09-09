use amiss_wire::controls::valid_required_status_name;
use amiss_wire::digest::Digest;
use serde::{Deserialize, Serialize};

use crate::{AcceptedDelivery, CheckBinding, ControllerEvaluationId};

use super::model::{
    StoredChange, StoredDelivery, StoredDeliveryIdentity, StoredProviderRun, StoredReplayKeep,
};
use super::publication::StoredPublication;
use crate::file_ledger::FileLedgerError;

const RECORD_SCHEMA: &str = "amiss/controller-file-record-v3";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct Record {
    schema: String,
    pub(in crate::file_ledger) generation: u64,
    pub(in crate::file_ledger) last_seen_unix_millis: i64,
    binding: StoredDelivery,
    replay_keep: StoredReplayKeep,
    check: CheckBinding,
    evaluation_id: String,
    pub(in crate::file_ledger) state: State,
}

impl Record {
    pub(in crate::file_ledger) fn running(
        delivery: &AcceptedDelivery,
        check: &CheckBinding,
        evaluation_id: &ControllerEvaluationId,
        owner: [u8; 16],
        now: i64,
        expires_at_unix_millis: i64,
    ) -> Self {
        let authenticated = delivery.delivery();
        Self {
            schema: RECORD_SCHEMA.to_owned(),
            generation: 1,
            last_seen_unix_millis: now,
            binding: StoredDelivery {
                identity: StoredDeliveryIdentity::new(&authenticated.identity),
                change: StoredChange::new(&authenticated.change),
                provider_run: StoredProviderRun {
                    run_id: authenticated.provider_run.run_id.as_str().to_owned(),
                    attempt: authenticated.provider_run.attempt.get(),
                    object_format: authenticated.provider_run.object_format,
                    candidate_commit: authenticated.provider_run.candidate_commit.clone(),
                },
            },
            replay_keep: StoredReplayKeep::new(delivery.replay_keep()),
            check: check.clone(),
            evaluation_id: evaluation_id.as_str().to_owned(),
            state: State::Running {
                owner,
                fence: 1,
                expires_at_unix_millis,
            },
        }
    }

    pub(in crate::file_ledger) fn matches(
        &self,
        delivery: &AcceptedDelivery,
        check: &CheckBinding,
    ) -> Result<bool, FileLedgerError> {
        Ok(self.binding.materialize()? == *delivery.delivery()
            && self.replay_keep == StoredReplayKeep::new(delivery.replay_keep())
            && self.check == *check)
    }

    pub(in crate::file_ledger) fn matches_key(&self, key: &str) -> Result<bool, FileLedgerError> {
        Ok(super::delivery_key(&self.binding.materialize()?.identity)? == key)
    }

    pub(in crate::file_ledger) fn evaluation_id(
        &self,
    ) -> Result<ControllerEvaluationId, FileLedgerError> {
        ControllerEvaluationId::new(self.evaluation_id.clone()).ok_or(FileLedgerError::Corrupt)
    }

    pub(in crate::file_ledger) fn advance(&mut self, now: i64) -> Result<(), FileLedgerError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(FileLedgerError::Corrupt)?;
        self.last_seen_unix_millis = now;
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), FileLedgerError> {
        if self.schema != RECORD_SCHEMA
            || self.generation == 0
            || self.last_seen_unix_millis < 0
            || !valid_required_status_name(&self.check.required_status_name)
        {
            return Err(FileLedgerError::Corrupt);
        }
        let delivery = self.binding.materialize()?;
        self.replay_keep.validate()?;
        if delivery.identity.provider != delivery.change.provider {
            return Err(FileLedgerError::Corrupt);
        }
        match &self.state {
            State::Running {
                fence,
                expires_at_unix_millis,
                ..
            } => {
                if *fence == 0
                    || *fence > self.generation
                    || *expires_at_unix_millis <= self.last_seen_unix_millis
                {
                    return Err(FileLedgerError::Corrupt);
                }
            }
            State::Staged { fence, publication } => {
                if *fence == 0 || *fence > self.generation {
                    return Err(FileLedgerError::Corrupt);
                }
                publication.validate_binding(&self.evaluation_id, &delivery, &self.check)?;
            }
            State::Done { fence, .. } => {
                if *fence == 0 || *fence > self.generation {
                    return Err(FileLedgerError::Corrupt);
                }
            }
        }
        Ok(())
    }

    pub(in crate::file_ledger) const fn is_done_and_expired(&self, now: i64) -> bool {
        matches!(self.state, State::Done { .. }) && self.replay_keep.expired_at(now)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub(in crate::file_ledger) enum State {
    Running {
        owner: [u8; 16],
        fence: u64,
        expires_at_unix_millis: i64,
    },
    Staged {
        fence: u64,
        publication: Box<StoredPublication>,
    },
    Done {
        fence: u64,
        staged_digest: Digest,
    },
}
