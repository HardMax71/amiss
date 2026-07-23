use std::path::Path;

use crate::delivery::StoredDelivery;
use crate::limits::StoredLimits;
use crate::record::{LeaseData, Record, State, Transition};
use crate::store::Store;
use crate::{Delivery, InboxError, InboxLimits, IncomingDelivery};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Stored,
    Duplicate,
}

pub enum ClaimOutcome {
    Claimed(ClaimedDelivery),
    Waiting { ready_at_unix_millis: i64 },
    Empty,
}

pub struct ClaimedDelivery {
    pub delivery: Delivery,
    pub lease: DeliveryLease,
}

#[derive(Clone)]
pub struct DeliveryLease {
    pub attempt: u64,
    pub expires_at_unix_millis: i64,
    key: String,
    owner: String,
    fence: u64,
}

pub enum RenewOutcome {
    Renewed(DeliveryLease),
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryOutcome {
    Scheduled,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompleteOutcome {
    Completed,
    Lost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxEntry {
    pub route: String,
    pub source_id: String,
    pub state: InboxState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxState {
    Pending {
        attempts: u64,
        available_at_unix_millis: i64,
    },
    Claimed {
        attempt: u64,
        expires_at_unix_millis: i64,
    },
}

/// A bounded raw-delivery directory with one active process owner.
///
/// The caller acknowledges a provider only after [`Self::enqueue`] returns
/// [`EnqueueOutcome::Stored`] or [`EnqueueOutcome::Duplicate`]. Completion
/// removes the raw row; the controller delivery ledger remains the replay and
/// final idempotence authority.
pub struct Inbox {
    store: Store,
    limits: StoredLimits,
    owner: String,
}

impl Inbox {
    /// Opens a dedicated, pre-existing private directory.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid limits, a second live owner, unknown or
    /// non-regular entries, corrupt rows, or inaccessible storage.
    pub fn open(root: impl AsRef<Path>, limits: InboxLimits) -> Result<Self, InboxError> {
        let limits = StoredLimits::read(limits)?;
        Ok(Self {
            store: Store::open(root.as_ref(), limits)?,
            limits,
            owner: random_owner()?,
        })
    }

    /// Persists one normalized raw delivery before returning.
    ///
    /// The source identity is the route and `source_id` pair. Repeating that
    /// pair with the same headers and body is a duplicate; different bytes are
    /// a conflict. Receipt time is intentionally not part of duplicate
    /// comparison.
    ///
    /// # Errors
    ///
    /// Returns an error when the delivery is invalid, capacity is exhausted,
    /// a source conflicts, or any stored row cannot be trusted.
    pub fn enqueue(
        &mut self,
        incoming: IncomingDelivery<'_>,
    ) -> Result<EnqueueOutcome, InboxError> {
        let delivery = StoredDelivery::read(incoming, self.limits)?;
        let key = delivery.key()?;
        let record = Record::pending(delivery)?;
        let entries = self.store.scan()?;
        if let Some(row) = entries.rows.get(&key) {
            return if row.record.content_digest == record.content_digest {
                Ok(EnqueueOutcome::Duplicate)
            } else {
                Err(InboxError::Conflict)
            };
        }
        self.store.save_new(&entries, &key, &record)?;
        Ok(EnqueueOutcome::Stored)
    }

    /// Claims the first ready delivery in stable source-key order.
    ///
    /// An expired claim becomes ready for a new attempt. `now` must be a
    /// non-negative Unix millisecond value from controller-owned time.
    ///
    /// # Errors
    ///
    /// Returns an error for untrusted time, corrupt storage, or capacity that
    /// cannot hold the atomic claim replacement.
    pub fn claim(&mut self, now: i64) -> Result<ClaimOutcome, InboxError> {
        valid_now(now)?;
        let entries = self.store.scan()?;
        let mut next_ready = None;
        let mut selected = None;
        for (key, row) in &entries.rows {
            let ready_at = row.record.ready_at();
            if ready_at <= now && selected.is_none() {
                selected = Some((key.clone(), row.bytes, row.record.clone()));
            } else {
                next_ready = Some(next_ready.map_or(ready_at, |next: i64| next.min(ready_at)));
            }
        }
        let Some((key, old_bytes, record)) = selected else {
            return Ok(
                next_ready.map_or(ClaimOutcome::Empty, |ready_at_unix_millis| {
                    ClaimOutcome::Waiting {
                        ready_at_unix_millis,
                    }
                }),
            );
        };
        let delivery = record
            .delivery
            .as_ref()
            .ok_or(InboxError::Corrupt)?
            .materialize(self.limits)?;
        let (record, lease) = record.claim(&self.owner, now, self.limits.lease_millis())?;
        self.store.replace(&entries, &key, &record, old_bytes)?;
        Ok(ClaimOutcome::Claimed(ClaimedDelivery {
            delivery,
            lease: make_lease(key, lease),
        }))
    }

    /// Extends a live lease and returns its replacement token.
    ///
    /// # Errors
    ///
    /// Returns an error for untrusted time or storage that cannot be trusted or
    /// atomically updated.
    pub fn renew(&mut self, lease: &DeliveryLease, now: i64) -> Result<RenewOutcome, InboxError> {
        valid_now(now)?;
        let entries = self.store.scan()?;
        let row = entries.rows.get(&lease.key).ok_or(InboxError::Corrupt)?;
        match row.record.clone().renew(
            &lease.owner,
            lease.fence,
            now,
            self.limits.lease_millis(),
        )? {
            Transition::Applied((record, renewed)) => {
                self.store
                    .replace(&entries, &lease.key, &record, row.bytes)?;
                Ok(RenewOutcome::Renewed(make_lease(
                    lease.key.clone(),
                    renewed,
                )))
            }
            Transition::Lost => Ok(RenewOutcome::Lost),
        }
    }

    /// Releases a live lease for another attempt at or after `available_at`.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid time or storage that cannot be trusted or
    /// atomically updated.
    pub fn retry(
        &mut self,
        lease: &DeliveryLease,
        now: i64,
        available_at_unix_millis: i64,
    ) -> Result<RetryOutcome, InboxError> {
        valid_now(now)?;
        if available_at_unix_millis < now {
            return Err(InboxError::Clock);
        }
        let entries = self.store.scan()?;
        let row = entries.rows.get(&lease.key).ok_or(InboxError::Corrupt)?;
        match row
            .record
            .clone()
            .retry(&lease.owner, lease.fence, now, available_at_unix_millis)?
        {
            Transition::Applied(record) => {
                self.store
                    .replace(&entries, &lease.key, &record, row.bytes)?;
                Ok(RetryOutcome::Scheduled)
            }
            Transition::Lost => Ok(RetryOutcome::Lost),
        }
    }

    /// Removes the raw row after the controller delivery ledger has completed.
    ///
    /// A crash before removal leaves a retryable row. Reprocessing that row is
    /// safe because the delivery ledger owns final duplicate detection.
    ///
    /// # Errors
    ///
    /// Returns an error for untrusted time or storage that cannot be trusted or
    /// removed.
    pub fn complete(
        &mut self,
        lease: &DeliveryLease,
        now: i64,
    ) -> Result<CompleteOutcome, InboxError> {
        valid_now(now)?;
        let entries = self.store.scan()?;
        let row = entries.rows.get(&lease.key).ok_or(InboxError::Corrupt)?;
        if !row.record.lease_is_live(&lease.owner, lease.fence, now) {
            return Ok(CompleteOutcome::Lost);
        }
        self.store.remove(&lease.key)?;
        Ok(CompleteOutcome::Completed)
    }

    /// Lists every live row without exposing its body or authentication
    /// headers.
    ///
    /// # Errors
    ///
    /// Returns an error when any root entry or row cannot be trusted.
    pub fn entries(&mut self) -> Result<Vec<InboxEntry>, InboxError> {
        self.store
            .scan()?
            .rows
            .into_values()
            .map(|row| {
                let state = match row.record.state {
                    State::Pending {
                        available_at_unix_millis,
                    } => InboxState::Pending {
                        attempts: row.record.attempts,
                        available_at_unix_millis,
                    },
                    State::Claimed {
                        expires_at_unix_millis,
                        ..
                    } => InboxState::Claimed {
                        attempt: row.record.attempts,
                        expires_at_unix_millis,
                    },
                };
                Ok(InboxEntry {
                    route: row.record.route,
                    source_id: row.record.source_id,
                    state,
                })
            })
            .collect()
    }
}

fn make_lease(key: String, lease: LeaseData) -> DeliveryLease {
    DeliveryLease {
        attempt: lease.attempt,
        expires_at_unix_millis: lease.expires_at_unix_millis,
        key,
        owner: lease.owner,
        fence: lease.fence,
    }
}

fn random_owner() -> Result<String, InboxError> {
    let high = u128::from(getrandom::u64().map_err(|_| InboxError::Random)?);
    let low = u128::from(getrandom::u64().map_err(|_| InboxError::Random)?);
    Ok(hex::encode(((high << u64::BITS) | low).to_be_bytes()))
}

fn valid_now(now: i64) -> Result<(), InboxError> {
    (now >= 0).then_some(()).ok_or(InboxError::Clock)
}
