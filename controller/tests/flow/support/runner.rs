use std::collections::VecDeque;

use amiss_controller::{HeartbeatOutcome, RunHeartbeat, RunRequest, Runner, RunnerOutcome};

pub(crate) struct FakeRunner {
    outcomes: VecDeque<RunnerOutcome>,
    pub(crate) requests: Vec<RunRequest>,
    pub(crate) heartbeat_renewals: usize,
    pub(crate) heartbeat_deadlines: Vec<i64>,
}

impl FakeRunner {
    pub(crate) fn new(outcome: RunnerOutcome) -> Self {
        Self {
            outcomes: VecDeque::from([outcome]),
            requests: Vec::new(),
            heartbeat_renewals: 0,
            heartbeat_deadlines: Vec::new(),
        }
    }
}

impl Runner for FakeRunner {
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        self.requests.push(request.clone());
        self.heartbeat_deadlines
            .push(heartbeat.expires_at_unix_millis());
        for _ in 0..self.heartbeat_renewals {
            match heartbeat.renew() {
                HeartbeatOutcome::Renewed {
                    expires_at_unix_millis,
                } => self.heartbeat_deadlines.push(expires_at_unix_millis),
                HeartbeatOutcome::Stop => return RunnerOutcome::Unavailable,
            }
        }
        self.outcomes
            .pop_front()
            .unwrap_or(RunnerOutcome::Unavailable)
    }
}
