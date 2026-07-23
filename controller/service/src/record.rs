use serde::{Deserialize, Serialize};

use crate::InboxError;
use crate::delivery::{StoredDelivery, source_key, validate_source};
use crate::hash::is_digest;
use crate::limits::StoredLimits;

const RECORD_SCHEMA: &str = "amiss/controller-inbox-record-v1";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    schema: String,
    pub(crate) route: String,
    pub(crate) source_id: String,
    pub(crate) content_digest: String,
    pub(crate) generation: u64,
    pub(crate) attempts: u64,
    pub(crate) fence: u64,
    pub(crate) delivery: Option<StoredDelivery>,
    pub(crate) state: State,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum State {
    Pending {
        available_at_unix_millis: i64,
    },
    Claimed {
        owner: String,
        expires_at_unix_millis: i64,
    },
}

pub(crate) struct LeaseData {
    pub(crate) owner: String,
    pub(crate) fence: u64,
    pub(crate) attempt: u64,
    pub(crate) expires_at_unix_millis: i64,
}

impl Record {
    pub(crate) fn pending(delivery: StoredDelivery) -> Result<Self, InboxError> {
        Ok(Self {
            schema: RECORD_SCHEMA.to_owned(),
            route: delivery.route().to_owned(),
            source_id: delivery.source_id().to_owned(),
            content_digest: delivery.content_digest()?,
            generation: 0,
            attempts: 0,
            fence: 0,
            delivery: Some(delivery),
            state: State::Pending {
                available_at_unix_millis: 0,
            },
        })
    }

    pub(crate) fn validate(&self, key: &str, limits: StoredLimits) -> Result<(), InboxError> {
        if self.schema != RECORD_SCHEMA
            || source_key(&self.route, &self.source_id)? != key
            || !is_digest(&self.content_digest)
            || self.attempts != self.fence
            || self.generation < self.attempts
        {
            return Err(InboxError::Corrupt);
        }
        validate_source(&self.route, &self.source_id, limits).map_err(|_| InboxError::Corrupt)?;
        match (&self.state, &self.delivery) {
            (
                State::Pending {
                    available_at_unix_millis,
                },
                Some(delivery),
            ) if *available_at_unix_millis >= 0 => self.validate_delivery(delivery, limits),
            (
                State::Claimed {
                    owner,
                    expires_at_unix_millis,
                },
                Some(delivery),
            ) if valid_owner(owner) && *expires_at_unix_millis > 0 && self.attempts > 0 => {
                self.validate_delivery(delivery, limits)
            }
            (State::Pending { .. } | State::Claimed { .. }, None | Some(_)) => {
                Err(InboxError::Corrupt)
            }
        }
    }

    pub(crate) fn ready_at(&self) -> i64 {
        match &self.state {
            State::Pending {
                available_at_unix_millis,
            } => *available_at_unix_millis,
            State::Claimed {
                expires_at_unix_millis,
                ..
            } => *expires_at_unix_millis,
        }
    }

    pub(crate) fn begin_attempt(self) -> Result<Self, InboxError> {
        let attempts = self.attempts.checked_add(1).ok_or(InboxError::Corrupt)?;
        let fence = self.fence.checked_add(1).ok_or(InboxError::Corrupt)?;
        Ok(Self {
            attempts,
            fence,
            ..self
        })
    }

    pub(crate) fn lease(
        self,
        owner: &str,
        now: i64,
        lease_millis: i64,
    ) -> Result<(Self, LeaseData), InboxError> {
        let expires_at_unix_millis = now.checked_add(lease_millis).ok_or(InboxError::Clock)?;
        let attempts = self.attempts;
        let fence = self.fence;
        self.claimed(owner, expires_at_unix_millis, attempts, fence)
    }

    pub(crate) fn retry(self, available_at_unix_millis: i64) -> Result<Self, InboxError> {
        let generation = self.generation.checked_add(1).ok_or(InboxError::Corrupt)?;
        Ok(Self {
            generation,
            state: State::Pending {
                available_at_unix_millis,
            },
            ..self
        })
    }

    pub(crate) fn lease_is_live(&self, owner: &str, fence: u64, now: i64) -> bool {
        self.lease_matches(owner, fence, now)
    }

    fn claimed(
        self,
        owner: &str,
        expires_at_unix_millis: i64,
        attempts: u64,
        fence: u64,
    ) -> Result<(Self, LeaseData), InboxError> {
        let generation = self.generation.checked_add(1).ok_or(InboxError::Corrupt)?;
        let owner = owner.to_owned();
        let lease = LeaseData {
            owner: owner.clone(),
            fence,
            attempt: attempts,
            expires_at_unix_millis,
        };
        let record = Self {
            generation,
            attempts,
            fence,
            state: State::Claimed {
                owner,
                expires_at_unix_millis,
            },
            ..self
        };
        Ok((record, lease))
    }

    fn validate_delivery(
        &self,
        delivery: &StoredDelivery,
        limits: StoredLimits,
    ) -> Result<(), InboxError> {
        let materialized = delivery.materialize(limits)?;
        if materialized.route != self.route
            || materialized.source_id != self.source_id
            || delivery.content_digest()? != self.content_digest
        {
            return Err(InboxError::Corrupt);
        }
        Ok(())
    }

    fn lease_matches(&self, owner: &str, fence: u64, now: i64) -> bool {
        matches!(
            &self.state,
            State::Claimed {
                owner: claimed_owner,
                expires_at_unix_millis,
            } if claimed_owner == owner
                && self.fence == fence
                && now < *expires_at_unix_millis
        )
    }
}

fn valid_owner(owner: &str) -> bool {
    owner.len() == 32
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
