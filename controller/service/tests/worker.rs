#![expect(
    clippy::unwrap_used,
    reason = "fixed worker fixtures and filesystem setup must fail loudly"
)]

#[path = "worker/support.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_controller_service::{ClaimOutcome, InboxState, WorkOutcome};

use support::{Fixture, Refresh, enqueue};

#[test]
fn admitted_row_is_reauthenticated_run_and_completed() {
    let mut fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue(&fixture.inbox, &fixture.admission);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.admission.calls.load(Ordering::Relaxed), 2);
    assert_eq!(fixture.adapter.authentications.load(Ordering::Relaxed), 1);
    assert_eq!(fixture.adapter.publications.load(Ordering::Relaxed), 1);
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
    enqueue(&fixture.inbox, &fixture.admission);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    let entries = fixture.inbox.lock().unwrap().entries().unwrap();
    assert!(matches!(
        entries.first().unwrap().state,
        InboxState::Pending { attempts: 1, .. }
    ));

    thread::sleep(Duration::from_millis(140));
    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.adapter.publications.load(Ordering::Relaxed), 1);
}

#[test]
fn failed_reauthentication_discards_the_raw_row() {
    let mut fixture = Fixture::new([Refresh::Active, Refresh::Active], Duration::ZERO);
    enqueue(&fixture.inbox, &fixture.admission);
    fixture.admission.accept.store(false, Ordering::Release);

    assert_eq!(fixture.worker.work_once().unwrap(), WorkOutcome::Processed);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
    assert_eq!(fixture.admission.calls.load(Ordering::Relaxed), 2);
    assert_eq!(fixture.adapter.authentications.load(Ordering::Relaxed), 0);
}

#[test]
fn renewal_keeps_a_long_controller_operation_owned() {
    let (fixture, release) = Fixture::held([Refresh::Active, Refresh::Active]);
    enqueue(&fixture.inbox, &fixture.admission);
    let inbox = Arc::clone(&fixture.inbox);
    let started = Arc::clone(&fixture.run_started);

    let worker = thread::spawn(move || {
        let mut worker = fixture.worker;
        worker.work_once()
    });
    started.wait();
    let first_expiry = claimed_expiry(&mut inbox.lock().unwrap());
    let observation_deadline = Instant::now() + Duration::from_secs(10);
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
