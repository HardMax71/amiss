#![expect(
    clippy::unwrap_used,
    reason = "fixed worker fixtures and filesystem setup must fail loudly"
)]

#[path = "worker/support.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_controller_service::{InboxState, WorkOutcome};

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
    let fixture = Fixture::new(
        [Refresh::Active, Refresh::Active],
        Duration::from_millis(300),
    );
    enqueue(&fixture.inbox, &fixture.admission);
    let inbox = Arc::clone(&fixture.inbox);
    let started = Arc::clone(&fixture.run_started);

    let worker = thread::spawn(move || {
        let mut worker = fixture.worker;
        worker.work_once()
    });
    started.wait();
    thread::sleep(Duration::from_millis(180));

    assert!(matches!(
        inbox.lock().unwrap().claim(support::now()).unwrap(),
        amiss_controller_service::ClaimOutcome::Waiting { .. }
    ));
    assert_eq!(worker.join().unwrap().unwrap(), WorkOutcome::Processed);
    assert!(inbox.lock().unwrap().entries().unwrap().is_empty());
}
