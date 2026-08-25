#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "scripted lease fixtures must fail loudly"
)]

use std::sync::mpsc;
use std::time::Duration;

use processkit::CancellationToken;

use std::path::Path;

use super::{
    directory, read_bounded, receive, regular_file, renewal_wait, reserve_bounded, valid_run,
};
use crate::BootstrapRun;
use crate::{BootstrapTermination, HeartbeatOutcome, RunHeartbeat};
use amiss_wire::model::UtcInstant;
use amiss_wire::report::WATCHDOG_MILLISECONDS;

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

fn instant(raw: &str) -> UtcInstant {
    UtcInstant::new(raw.to_owned()).unwrap()
}

fn run<'a>(
    executable: &'a Path,
    directory: &'a Path,
    wall_timeout: Duration,
    evaluation_instant: &'a UtcInstant,
    valid_until: &'a UtcInstant,
) -> BootstrapRun<'a> {
    BootstrapRun {
        executable,
        repository: directory,
        action_repository: directory,
        scratch: directory,
        evaluation_instant,
        valid_until,
        semantic_evidence: &[],
        wall_timeout,
    }
}

#[test]
fn a_run_is_valid_only_within_the_watchdog_and_over_real_paths() {
    let root = tempfile::TempDir::new().unwrap();
    let executable = root.path().join("bootstrap");
    std::fs::write(&executable, b"#!/bin/sh\n").unwrap();
    let evaluation = instant("2026-07-01T00:00:00Z");
    let until = instant("2026-07-01T00:10:00Z");
    let watchdog = Duration::from_millis(WATCHDOG_MILLISECONDS);

    assert!(valid_run(&run(
        &executable,
        root.path(),
        watchdog,
        &evaluation,
        &until
    )));
    assert!(
        !valid_run(&run(
            &executable,
            root.path(),
            watchdog.saturating_add(Duration::from_millis(1)),
            &evaluation,
            &until
        )),
        "a timeout past the watchdog"
    );
    assert!(
        !valid_run(&run(
            &executable,
            root.path(),
            Duration::ZERO,
            &evaluation,
            &until
        )),
        "no time at all"
    );
    assert!(
        !valid_run(&run(
            root.path(),
            root.path(),
            watchdog,
            &evaluation,
            &until
        )),
        "a directory where the executable belongs"
    );
    assert!(
        !valid_run(&run(
            &executable,
            &executable,
            watchdog,
            &evaluation,
            &until
        )),
        "a file where a directory belongs"
    );
}

#[test]
fn a_path_must_be_absolute_and_of_its_own_kind() {
    let root = tempfile::TempDir::new().unwrap();
    let file = root.path().join("bootstrap");
    std::fs::write(&file, b"#!/bin/sh\n").unwrap();

    assert!(regular_file(&file));
    assert!(!regular_file(root.path()), "a directory is not a file");
    assert!(
        !regular_file(Path::new("bootstrap")),
        "a relative path names nothing certain"
    );
    assert!(directory(root.path()));
    assert!(!directory(&file), "a file is not a directory");
    assert!(
        !directory(Path::new("scratch")),
        "a relative path names nothing certain"
    );
}

#[test]
fn a_bounded_read_admits_exactly_its_limit() {
    let root = tempfile::TempDir::new().unwrap();
    let path = root.path().join("payload");
    std::fs::write(&path, b"0123456789").unwrap();

    assert_eq!(read_bounded(&path, 10).unwrap(), b"0123456789");
    assert!(read_bounded(&path, 9).is_err(), "one byte past the limit");
    assert_eq!(read_bounded(&path, 64).unwrap().len(), 10);
}

#[test]
fn a_reservation_stops_at_its_bound() {
    let mut bytes = Vec::new();
    assert!(reserve_bounded(&mut bytes, 8, 16).is_ok());
    assert!(bytes.capacity() >= 8);
    let mut full = Vec::new();
    assert!(
        reserve_bounded(&mut full, 32, 16).is_err(),
        "a reservation past the bound"
    );
}
