use std::collections::VecDeque;
use std::time::Duration;

use amiss_controller::{HeartbeatOutcome, RunHeartbeat, RunRequest, Runner, RunnerOutcome};

pub(crate) struct FakeRunner {
    outcomes: VecDeque<RunnerOutcome>,
    pub(crate) requests: Vec<RunRequest>,
    pub(crate) heartbeat_renewals: usize,
    pub(crate) heartbeat_windows: Vec<Duration>,
}

impl FakeRunner {
    pub(crate) fn new(outcome: RunnerOutcome) -> Self {
        Self {
            outcomes: VecDeque::from([outcome]),
            requests: Vec::new(),
            heartbeat_renewals: 0,
            heartbeat_windows: Vec::new(),
        }
    }
}

impl Runner for FakeRunner {
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        self.requests.push(request.clone());
        for _ in 0..self.heartbeat_renewals {
            match heartbeat.renew() {
                HeartbeatOutcome::Renewed { renew_within } => {
                    self.heartbeat_windows.push(renew_within);
                }
                HeartbeatOutcome::Stop => return RunnerOutcome::Unavailable,
            }
        }
        self.outcomes
            .pop_front()
            .unwrap_or(RunnerOutcome::Unavailable)
    }
}
