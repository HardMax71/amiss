mod format;
mod store;
mod transitions;

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use format::Record;
use store::{Row, Store};

use crate::{
    AcceptedDelivery, ControllerClock, ControllerEvaluationId, DeliveryIdentity, ReplayWindow,
    SystemClock,
};

#[derive(Debug)]
pub enum FileLedgerError {
    Configuration,
    Full,
    Expired,
    Clock,
    Random,
    ReportTooLarge,
    Corrupt,
    Io(io::Error),
}

impl fmt::Display for FileLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration => formatter.write_str("delivery record configuration differs"),
            Self::Full => formatter.write_str("delivery record capacity is full"),
            Self::Expired => formatter.write_str("delivery replay lifetime has ended"),
            Self::Clock => formatter.write_str("controller time cannot be trusted"),
            Self::Random => formatter.write_str("controller owner identity could not be created"),
            Self::ReportTooLarge => formatter.write_str("saved report exceeds the report ceiling"),
            Self::Corrupt => formatter.write_str("delivery record is corrupt"),
            Self::Io(error) => write!(formatter, "delivery record I/O failed: {error}"),
        }
    }
}

impl std::error::Error for FileLedgerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Configuration
            | Self::Full
            | Self::Expired
            | Self::Clock
            | Self::Random
            | Self::ReportTooLarge
            | Self::Corrupt => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileLedgerCleanup {
    /// Completed delivery records removed after their replay lifetime.
    pub removed_records: u64,
    /// Report files no saved publication still needs.
    pub removed_reports: u64,
    /// Recognized leftovers from interrupted atomic writes.
    pub removed_temporary: u64,
}

/// Lease timing plus the fixed admission and replay limits for one record root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileLedgerConfig {
    lease_millis: i64,
    max_records: u64,
    replay_window: ReplayWindow,
}

impl FileLedgerConfig {
    /// Returns `None` for a zero or unrepresentable lease or a zero record cap.
    pub fn new(
        lease_duration: Duration,
        max_records: u64,
        replay_window: ReplayWindow,
    ) -> Option<Self> {
        let lease_millis = i64::try_from(lease_duration.as_millis())
            .ok()
            .filter(|millis| *millis > 0)?;
        (max_records > 0).then_some(Self {
            lease_millis,
            max_records,
            replay_window,
        })
    }

    pub const fn max_records(self) -> u64 {
        self.max_records
    }

    pub const fn replay_window(self) -> ReplayWindow {
        self.replay_window
    }
}

impl From<io::Error> for FileLedgerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<crate::atomic_write_recovery::AtomicWriteDirectoryError> for FileLedgerError {
    fn from(error: crate::atomic_write_recovery::AtomicWriteDirectoryError) -> Self {
        match error {
            crate::atomic_write_recovery::AtomicWriteDirectoryError::Io(error) => Self::Io(error),
            crate::atomic_write_recovery::AtomicWriteDirectoryError::Malformed => Self::Corrupt,
        }
    }
}

/// A provider-neutral delivery record backed by atomic files and operating
/// system file locks.
///
/// The root must be a private local directory controlled by the service, not
/// by a repository or action tree. Replay-only completion markers are retained
/// indefinitely; bounded markers are removable only after their authenticated
/// replay lifetime ends.
pub struct FileLedger {
    store: Store,
    lease_millis: i64,
    owner: [u8; 16],
    clock: Arc<dyn ControllerClock>,
}

impl FileLedger {
    /// Opens an existing record directory with the system clock.
    ///
    /// # Errors
    ///
    /// Returns an error when trusted time, owner randomness, root metadata, or
    /// the root directory cannot be validated.
    pub fn open(root: impl AsRef<Path>, config: FileLedgerConfig) -> Result<Self, FileLedgerError> {
        Self::open_with_clock(root, config, Arc::new(SystemClock))
    }

    /// Opens a record directory with an injected controller-owned clock.
    ///
    /// # Errors
    ///
    /// Returns an error under the same conditions as [`Self::open`].
    pub fn open_with_clock(
        root: impl AsRef<Path>,
        config: FileLedgerConfig,
        clock: Arc<dyn ControllerClock>,
    ) -> Result<Self, FileLedgerError> {
        let lease_millis = config.lease_millis;
        let now = clock
            .now_unix_millis()
            .filter(|now| *now >= 0)
            .ok_or(FileLedgerError::Clock)?;
        let ledger = Self {
            store: Store::open(root.as_ref(), config, now)?,
            lease_millis,
            owner: random_id()?,
            clock,
        };
        ledger.cleanup()?;
        Ok(ledger)
    }

    fn now(&self, row: &Row, record: Option<&Record>) -> Result<i64, FileLedgerError> {
        let now = self
            .clock
            .now_unix_millis()
            .filter(|now| *now >= 0)
            .ok_or(FileLedgerError::Clock)?;
        let now = row.observe_clock(now)?;
        Ok(record.map_or(now, |record| now.max(record.last_seen_unix_millis)))
    }

    fn new_evaluation_id(
        identity: &DeliveryIdentity,
    ) -> Result<ControllerEvaluationId, FileLedgerError> {
        format::evaluation_id(identity, &random_id()?)
    }

    fn deadline(&self, now: i64) -> Result<i64, FileLedgerError> {
        now.checked_add(self.lease_millis)
            .ok_or(FileLedgerError::Clock)
    }

    fn accepted_expired(
        &self,
        row: &Row,
        delivery: &AcceptedDelivery,
    ) -> Result<bool, FileLedgerError> {
        let now = self.now(row, None)?;
        Ok(delivery
            .replay_keep_through_unix_millis()
            .is_some_and(|keep_through| now > keep_through))
    }

    fn row(&self, delivery: &AcceptedDelivery) -> Result<Row, FileLedgerError> {
        self.store.lock(
            &format::delivery_key(&delivery.delivery().identity)?,
            delivery.replay_keep(),
        )
    }

    /// Removes dead report and temporary files and completed records whose
    /// authenticated replay lifetime has ended.
    ///
    /// # Errors
    ///
    /// Returns an error when trusted time, root metadata, saved records, or a
    /// cleanup operation cannot be validated.
    pub fn cleanup(&self) -> Result<FileLedgerCleanup, FileLedgerError> {
        let now = self
            .clock
            .now_unix_millis()
            .filter(|now| *now >= 0)
            .ok_or(FileLedgerError::Clock)?;
        self.store.cleanup(now)
    }
}

fn random_id() -> Result<[u8; 16], FileLedgerError> {
    let high = u128::from(getrandom::u64().map_err(|_| FileLedgerError::Random)?);
    let low = u128::from(getrandom::u64().map_err(|_| FileLedgerError::Random)?);
    Ok(((high << u64::BITS) | low).to_be_bytes())
}
