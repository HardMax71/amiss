use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

use amiss_controller::{ExternalSink, ExternalTally};

/// Records every sink call for assertion; the flow tests own the counts.
#[derive(Default)]
pub(crate) struct RecordingSink {
    pub(crate) tallies: Mutex<Vec<ExternalTally>>,
    pub(crate) incomplete: AtomicUsize,
}

impl ExternalSink for RecordingSink {
    fn assessed(&self, tally: &ExternalTally) {
        self.tallies.lock().unwrap().push(*tally);
    }

    fn incomplete(&self) {
        self.incomplete
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
