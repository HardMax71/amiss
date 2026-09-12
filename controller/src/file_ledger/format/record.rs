#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum RecordSchema {
    #[serde(rename = "amiss/controller-file-record-v3")]
    Current,
}

use amiss_wire::model::Digest;
use serde::{Deserialize, Serialize};

use crate::{AcceptedDelivery, CheckBinding, ControllerEvaluationId};

use super::model::{StoredDelivery, StoredReplayKeep};
use super::publication::StoredPublication;
use crate::file_ledger::FileLedgerError;

const RECORD_SCHEMA: RecordSchema = RecordSchema::Current;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct Record {
    schema: RecordSchema,
    pub(in crate::file_ledger) generation: u64,
    pub(in crate::file_ledger) last_seen_unix_millis: i64,
    binding: StoredDelivery,
    replay_keep: StoredReplayKeep,
    check: CheckBinding,
    evaluation_id: ControllerEvaluationId,
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
        Self {
            schema: RECORD_SCHEMA,
            generation: 1,
            last_seen_unix_millis: now,
            binding: StoredDelivery::new(delivery.delivery()),
            replay_keep: StoredReplayKeep::new(delivery.replay_keep()),
            check: check.clone(),
            evaluation_id: evaluation_id.clone(),
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
    ) -> bool {
        self.binding.matches(delivery.delivery())
            && self.replay_keep == StoredReplayKeep::new(delivery.replay_keep())
            && self.check == *check
    }

    pub(in crate::file_ledger) fn matches_key(&self, key: &str) -> Result<bool, FileLedgerError> {
        Ok(super::delivery_key(&self.binding.identity)? == key)
    }

    pub(in crate::file_ledger) fn evaluation_id(&self) -> ControllerEvaluationId {
        self.evaluation_id.clone()
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
        self.binding.validate()?;
        self.replay_keep.validate()?;
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
                publication.validate_binding(&self.evaluation_id, &self.binding, &self.check)?;
            }
            State::Done {
                fence,
                staged_digest: _,
            } => {
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

#[derive(Serialize, Deserialize)]
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
