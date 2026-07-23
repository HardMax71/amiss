#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "scripted lease fixtures must fail loudly"
)]

use std::sync::mpsc;
use std::time::Duration;

use processkit::CancellationToken;

use super::{receive, renewal_wait};
use crate::{BootstrapTermination, HeartbeatOutcome, RunHeartbeat};

type Delivery = std::io::Result<BootstrapTermination>;

enum Step {
    DeliverThenRenew(mpsc::SyncSender<Delivery>, BootstrapTermination, Duration),
    DeliverThenStop(mpsc::SyncSender<Delivery>, BootstrapTermination),
}

struct Script {
    calls: u64,
    steps: Vec<Step>,
}

impl Script {
    fn new(steps: Vec<Step>) -> Self {
        Self { calls: 0, steps }
    }
}

impl RunHeartbeat for Script {
    fn renew(&mut self) -> HeartbeatOutcome {
        self.calls = self.calls.saturating_add(1);
        match self.steps.remove(0) {
            Step::DeliverThenRenew(sender, termination, renew_within) => {
                sender.send(Ok(termination)).unwrap();
                HeartbeatOutcome::Renewed { renew_within }
            }
            Step::DeliverThenStop(sender, termination) => {
                sender.send(Ok(termination)).unwrap();
                HeartbeatOutcome::Stop
            }
        }
    }
}

const ELAPSE_AT_ONCE: Duration = Duration::from_millis(1);

#[test]
fn a_delivered_termination_needs_no_renewal() {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender.send(Ok(BootstrapTermination::Exited(0))).unwrap();
    let cancelled = CancellationToken::new();
    let mut heartbeat = Script::new(vec![]);

    let outcome = receive(
        &receiver,
        &cancelled,
        Duration::from_mins(1),
        &mut heartbeat,
    );

    assert_eq!(outcome.unwrap().unwrap(), BootstrapTermination::Exited(0));
    assert_eq!(heartbeat.calls, 0);
    assert!(!cancelled.is_cancelled());
}

#[test]
fn an_elapsed_window_renews_exactly_once_before_delivery() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancelled = CancellationToken::new();
    let mut heartbeat = Script::new(vec![Step::DeliverThenRenew(
        sender,
        BootstrapTermination::Exited(0),
        Duration::from_mins(1),
    )]);

    let outcome = receive(&receiver, &cancelled, ELAPSE_AT_ONCE, &mut heartbeat);

    assert_eq!(outcome.unwrap().unwrap(), BootstrapTermination::Exited(0));
    assert_eq!(heartbeat.calls, 1);
    assert!(!cancelled.is_cancelled());
}

#[test]
fn a_stopped_lease_cancels_and_discards_a_delivered_termination() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancelled = CancellationToken::new();
    let mut heartbeat = Script::new(vec![Step::DeliverThenStop(
        sender,
        BootstrapTermination::Exited(0),
    )]);

    let outcome = receive(&receiver, &cancelled, ELAPSE_AT_ONCE, &mut heartbeat);

    assert_eq!(
        outcome.unwrap().unwrap(),
        BootstrapTermination::HeartbeatStopped
    );
    assert_eq!(heartbeat.calls, 1);
    assert!(cancelled.is_cancelled());
}

#[test]
fn a_renewal_window_keeps_the_wait_bounded_and_nonzero() {
    let one_hour = HeartbeatOutcome::Renewed {
        renew_within: Duration::from_hours(1),
    };
    assert_eq!(renewal_wait(one_hour), Some(Duration::from_secs(5)));

    let short = HeartbeatOutcome::Renewed {
        renew_within: Duration::from_millis(50),
    };
    assert_eq!(renewal_wait(short), Some(Duration::from_millis(25)));

    let empty = HeartbeatOutcome::Renewed {
        renew_within: Duration::ZERO,
    };
    assert_eq!(renewal_wait(empty), None);
    assert_eq!(renewal_wait(HeartbeatOutcome::Stop), None);
}

#[test]
fn a_closed_channel_is_a_supervision_defect() {
    let (sender, receiver) = mpsc::sync_channel::<Delivery>(1);
    drop(sender);
    let cancelled = CancellationToken::new();
    let mut heartbeat = Script::new(vec![]);

    let outcome = receive(
        &receiver,
        &cancelled,
        Duration::from_mins(1),
        &mut heartbeat,
    );

    assert!(outcome.is_err());
    assert_eq!(heartbeat.calls, 0);
}
