#![expect(
    clippy::unwrap_used,
    reason = "fixed worker fixtures and filesystem setup must fail loudly"
)]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_controller_service::{ClaimOutcome, InboxState, WorkOutcome};

use support::{Fixture, Refresh, SOURCE_ID, enqueue, enqueue_stored};

#[test]
fn admitted_row_is_reauthenticated_run_and_completed() {
    let mut fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.admission.calls.load(Ordering::Relaxed), 2);
    assert_eq!(fixture.adapter.authentications.load(Ordering::Relaxed), 1);
    assert_eq!(fixture.adapter.publications.load(Ordering::Relaxed), 1);
    assert_eq!(fixture.operations.delivery_attempts.get(), 1);
    assert_eq!(fixture.operations.delivery_completions.get(), 1);
    assert_eq!(fixture.operations.delivery_retries.get(), 0);
    assert_eq!(fixture.operations.delivery_discards.get(), 0);
}

#[test]
fn transient_provider_failure_is_retried() {
    let mut fixture = Fixture::new(
        [
            Refresh::Error(ProviderError::Unavailable),
            Refresh::Active,
            Refresh::Active,
        ],
        Duration::ZERO,
    );
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    let entries = fixture.inbox.lock().unwrap().entries().unwrap();
    assert!(matches!(
        entries.first().unwrap().state,
        InboxState::Pending { attempts: 1, .. }
    ));
    assert_eq!(fixture.operations.delivery_attempts.get(), 1);
    assert_eq!(fixture.operations.delivery_retries.get(), 1);
    assert_eq!(fixture.operations.delivery_completions.get(), 0);

    thread::sleep(Duration::from_millis(140));
    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.adapter.publications.load(Ordering::Relaxed), 1);
    assert_eq!(fixture.operations.delivery_attempts.get(), 2);
    assert_eq!(fixture.operations.delivery_completions.get(), 1);
}

#[test]
fn failed_reauthentication_discards_the_raw_row() {
    let mut fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);
    fixture.admission.accept.store(false, Ordering::Release);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.admission.calls.load(Ordering::Relaxed), 2);
    assert_eq!(fixture.adapter.authentications.load(Ordering::Relaxed), 0);
    assert_eq!(fixture.operations.delivery_attempts.get(), 1);
    assert_eq!(fixture.operations.delivery_completions.get(), 1);
    assert_eq!(fixture.operations.delivery_discards.get(), 1);
}

#[test]
fn a_row_stored_under_a_foreign_source_is_discarded() {
    let mut fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue_stored(&fixture.inbox, &fixture.admission, SOURCE_ID, "foreign");

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.adapter.authentications.load(Ordering::Relaxed), 0);
    assert_eq!(fixture.operations.delivery_discards.get(), 1);
}

#[test]
fn renewal_keeps_a_long_controller_operation_owned() {
    let (fixture, release) = Fixture::held([Refresh::Active, Refresh::Active]);
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);
    let inbox = Arc::clone(&fixture.inbox);
    let started = Arc::clone(&fixture.run_started);

    let worker = thread::spawn(move || {
        let mut worker = fixture.worker;
        worker.work_once()
    });
    started.wait();
    let first_expiry = claimed_expiry(&mut inbox.lock().unwrap());
    // Exits on the first observed renewal, around the five-second poll cap;
    // the wide ceiling is only for stalled runners.
    let observation_deadline = Instant::now() + Duration::from_secs(30);
    let renewed_and_owned = loop {
        let now = support::now();
        let mut inbox = inbox.lock().unwrap();
        let renewed = claimed_expiry(&mut inbox) > first_expiry;
        if renewed {
            break matches!(
                inbox.claim(now).unwrap(),
                ClaimOutcome::Waiting {
                    ready_at_unix_millis
                } if ready_at_unix_millis > now
            );
        }
        drop(inbox);
        if Instant::now() >= observation_deadline {
            break false;
        }
        thread::sleep(Duration::from_millis(1));
    };
    release.store(true, Ordering::Release);

    assert!(renewed_and_owned);
    assert_eq!(worker.join().unwrap().unwrap(), WorkOutcome::Processed);
    assert!(inbox.lock().unwrap().entries().unwrap().is_empty());
}

fn claimed_expiry(inbox: &mut amiss_controller_service::Inbox) -> i64 {
    match inbox.entries().unwrap().first().unwrap().state {
        InboxState::Claimed {
            expires_at_unix_millis,
            ..
        } => expires_at_unix_millis,
        InboxState::Pending { .. } => 0,
    }
}

#[test]
fn stop_finishes_the_current_operation_and_preserves_the_backlog() {
    let (fixture, release) = Fixture::held([
        Refresh::Active,
        Refresh::Active,
        Refresh::Active,
        Refresh::Active,
    ]);
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);
    enqueue(&fixture.inbox, &fixture.admission, "source-2");
    let inbox = Arc::clone(&fixture.inbox);
    let started = Arc::clone(&fixture.run_started);
    let stop = Arc::new(AtomicBool::new(false));
    let observed_stop = Arc::clone(&stop);
    let worker = thread::spawn(move || fixture.worker.run(&observed_stop));

    started.wait();
    stop.store(true, Ordering::Release);
    release.store(true, Ordering::Release);

    worker.join().unwrap().unwrap();
    let entries = inbox.lock().unwrap().entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries.first().unwrap().state,
        InboxState::Pending { .. }
    ));
    assert_eq!(fixture.operations.delivery_attempts.get(), 1);
    assert_eq!(fixture.operations.delivery_completions.get(), 1);
}

#[test]
fn stop_before_claim_preserves_the_backlog() {
    let fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue(&fixture.inbox, &fixture.admission, SOURCE_ID);
    let stop = AtomicBool::new(false);
    {
        let _inbox = fixture.inbox.lock().unwrap();
        stop.store(true, Ordering::Release);
    }

    fixture.worker.run(&stop).unwrap();

    assert_eq!(fixture.inbox.lock().unwrap().entries().unwrap().len(), 1);
    assert_eq!(fixture.operations.delivery_attempts.get(), 0);
}
