use std::time::{SystemTime, UNIX_EPOCH};

/// Supplies controller-owned wall time to trust and lease decisions.
pub trait ControllerClock: Send + Sync {
    /// Returns milliseconds since the Unix epoch, or `None` when time cannot
    /// be trusted.
    fn now_unix_millis(&self) -> Option<i64>;
}

/// The production wall clock used by the controller.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl ControllerClock for SystemClock {
    fn now_unix_millis(&self) -> Option<i64> {
        let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        i64::try_from(elapsed.as_millis()).ok()
    }
}
