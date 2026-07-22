use amiss_wire::digest::Digest;
use serde::{Deserialize, Serialize};

use crate::{AcceptedDelivery, ControllerEvaluationId};

use super::model::{StoredDelivery, StoredReplayKeep};
use super::publication::StoredPublication;
use crate::file_ledger::FileLedgerError;

const RECORD_SCHEMA: &str = "amiss/controller-file-record-v2";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct Record {
    schema: String,
    pub(in crate::file_ledger) generation: u64,
    pub(in crate::file_ledger) last_seen_unix_millis: i64,
    binding: StoredDelivery,
    replay_keep: StoredReplayKeep,
    evaluation_id: String,
    pub(in crate::file_ledger) state: State,
}

impl Record {
    pub(in crate::file_ledger) fn running(
        delivery: &AcceptedDelivery,
        evaluation_id: &ControllerEvaluationId,
        owner: [u8; 16],
        now: i64,
        expires_at_unix_millis: i64,
    ) -> Self {
        Self {
            schema: RECORD_SCHEMA.to_owned(),
            generation: 1,
            last_seen_unix_millis: now,
            binding: StoredDelivery::new(delivery.delivery()),
            replay_keep: StoredReplayKeep::new(delivery.replay_keep()),
            evaluation_id: evaluation_id.as_str().to_owned(),
            state: State::Running {
                owner,
                fence: 1,
                expires_at_unix_millis,
            },
        }
    }

    pub(in crate::file_ledger) fn matches(&self, delivery: &AcceptedDelivery) -> bool {
        self.binding == StoredDelivery::new(delivery.delivery())
            && self.replay_keep == StoredReplayKeep::new(delivery.replay_keep())
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
        if self.schema != RECORD_SCHEMA || self.generation == 0 || self.last_seen_unix_millis < 0 {
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
                let publication = publication.materialize_metadata()?;
                if publication.evaluation_id.as_str() != self.evaluation_id
                    || publication.provider_run != delivery.provider_run
                    || publication.run.change != delivery.change
                    || publication.run.object_format != delivery.provider_run.object_format
                    || publication.run.commits.candidate != delivery.provider_run.candidate_commit
                {
                    return Err(FileLedgerError::Corrupt);
                }
            }
            State::Done {
                fence,
                staged_digest,
            } => {
                if *fence == 0
                    || *fence > self.generation
                    || Digest::from_wire(staged_digest).is_none()
                {
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
        staged_digest: String,
    },
}
