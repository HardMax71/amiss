use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use amiss_controller::ControllerClock;

/// The one controller clock every test uses.
#[derive(Debug)]
pub struct TestClock {
    millis: AtomicI64,
    trusted: AtomicBool,
}

impl TestClock {
    /// Far enough from either i64 edge that a test may add or subtract freely.
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
