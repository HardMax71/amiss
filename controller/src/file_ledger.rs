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

use crate::{AuthenticatedDelivery, ControllerClock, SystemClock};

const OWNER_BYTES: usize = 16;

#[derive(Debug)]
pub enum FileLedgerError {
    InvalidLease,
    Clock,
    Random,
    ReportTooLarge,
    Corrupt,
    Io(io::Error),
}

impl fmt::Display for FileLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLease => formatter.write_str("delivery lease duration is invalid"),
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
            Self::InvalidLease
            | Self::Clock
            | Self::Random
            | Self::ReportTooLarge
            | Self::Corrupt => None,
        }
    }
}

impl From<io::Error> for FileLedgerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A provider-neutral delivery record backed by atomic files and operating
/// system file locks.
///
/// The root must be a private local directory controlled by the service, not
/// by a repository or action tree. Completed records are retained indefinitely.
pub struct FileLedger {
    store: Store,
    lease_millis: i64,
    owner: [u8; OWNER_BYTES],
    clock: Arc<dyn ControllerClock>,
}

impl FileLedger {
    /// Opens an existing record directory with the system clock.
    ///
    /// # Errors
    ///
    /// Returns an error when the lease is invalid, owner randomness is
    /// unavailable, or the root is not an existing real directory.
    pub fn open(root: impl AsRef<Path>, lease_duration: Duration) -> Result<Self, FileLedgerError> {
        Self::open_with_clock(root, lease_duration, Arc::new(SystemClock))
    }

    /// Opens a record directory with an injected controller-owned clock.
    ///
    /// # Errors
    ///
    /// Returns an error under the same conditions as [`Self::open`].
    pub fn open_with_clock(
        root: impl AsRef<Path>,
        lease_duration: Duration,
        clock: Arc<dyn ControllerClock>,
    ) -> Result<Self, FileLedgerError> {
        let lease_millis = i64::try_from(lease_duration.as_millis())
            .ok()
            .filter(|duration| *duration > 0)
            .ok_or(FileLedgerError::InvalidLease)?;
        let mut owner = [0_u8; OWNER_BYTES];
        getrandom::fill(&mut owner).map_err(|_| FileLedgerError::Random)?;
        Ok(Self {
            store: Store::open(root.as_ref())?,
            lease_millis,
            owner,
            clock,
        })
    }

    fn now(&self, record: Option<&Record>) -> Result<i64, FileLedgerError> {
        let now = self
            .clock
            .now_unix_millis()
            .filter(|now| *now >= 0)
            .ok_or(FileLedgerError::Clock)?;
        Ok(record.map_or(now, |record| now.max(record.last_seen_unix_millis)))
    }

    fn deadline(&self, now: i64) -> Result<i64, FileLedgerError> {
        now.checked_add(self.lease_millis)
            .ok_or(FileLedgerError::Clock)
    }

    fn row(&self, delivery: &AuthenticatedDelivery) -> Result<Row, FileLedgerError> {
        self.store.lock(&format::delivery_key(&delivery.identity)?)
    }
}
