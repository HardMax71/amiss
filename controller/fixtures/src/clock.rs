use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use amiss_controller::ControllerClock;

/// The one controller clock every test uses. It covers both axes the trait has:
/// an instant a test sets rather than waits for, and whether time is trusted at
/// all, which is the case no time-mocking crate models.
#[derive(Debug)]
pub struct TestClock {
    millis: AtomicI64,
    trusted: AtomicBool,
}

impl TestClock {
    /// A fixed instant well inside the representable range, for a test that
    /// needs a clock but not a particular reading.
    pub const DEFAULT: i64 = 1_800_000_000_000;

    #[must_use]
    pub fn new() -> Arc<Self> {
        Self::at(Self::DEFAULT)
    }

    #[must_use]
    pub fn at(millis: i64) -> Arc<Self> {
        Arc::new(Self {
            millis: AtomicI64::new(millis),
            trusted: AtomicBool::new(true),
        })
    }

    /// A clock that answers that time cannot be trusted.
    #[must_use]
    pub fn untrusted() -> Arc<Self> {
        let clock = Self::at(Self::DEFAULT);
        clock.distrust();
        clock
    }

    pub fn set(&self, millis: i64) {
        self.millis.store(millis, Ordering::SeqCst);
    }

    pub fn advance(&self, millis: i64) {
        self.millis.fetch_add(millis, Ordering::SeqCst);
    }

    pub fn distrust(&self) {
        self.trusted.store(false, Ordering::SeqCst);
    }

    #[must_use]
    pub fn now(&self) -> i64 {
        self.millis.load(Ordering::SeqCst)
    }
}

impl ControllerClock for TestClock {
    fn now_unix_millis(&self) -> Option<i64> {
        self.trusted
            .load(Ordering::SeqCst)
            .then(|| self.millis.load(Ordering::SeqCst))
    }
}
