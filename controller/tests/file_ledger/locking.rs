use std::sync::{Arc, Barrier};
use std::thread;

use amiss_controller::{DeliveryClaim, DeliveryLedger};
use tempfile::TempDir;

use super::support::{FIXTURE_EVALUATION, FIXTURE_KEY, TestClock, delivery, executed, open};

#[test]
fn delivery_identity_has_a_stable_evaluation_and_disk_key() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery("42")).unwrap()).unwrap();

    assert_eq!(lease.evaluation_id.as_str(), FIXTURE_EVALUATION);
    assert_eq!(FIXTURE_KEY.len(), 64);
    assert!(
        FIXTURE_KEY
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    );
    assert!(
        directory
            .path()
            .join(format!("{FIXTURE_KEY}.lock"))
            .is_file()
    );
    assert!(
        directory
            .path()
            .join(format!("{FIXTURE_KEY}.state"))
            .is_file()
    );
}

#[test]
fn concurrent_first_claims_choose_one_owner() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let barrier = Arc::new(Barrier::new(2));
    let mut first = open(directory.path(), &clock);
    let mut second = open(directory.path(), &clock);
    let first_delivery = delivery("42");
    let second_delivery = first_delivery.clone();
    let first_barrier = Arc::clone(&barrier);
    let second_barrier = Arc::clone(&barrier);

    let first_thread = thread::spawn(move || {
        first_barrier.wait();
        first.claim(&first_delivery)
    });
    let second_thread = thread::spawn(move || {
        second_barrier.wait();
        second.claim(&second_delivery)
    });
    let claims = [
        first_thread.join().unwrap().unwrap(),
        second_thread.join().unwrap().unwrap(),
    ];

    assert_eq!(
        claims
            .iter()
            .filter(|claim| matches!(claim, DeliveryClaim::Execute(_)))
            .count(),
        1
    );
    assert_eq!(
        claims
            .iter()
            .filter(|claim| matches!(claim, DeliveryClaim::Busy { .. }))
            .count(),
        1
    );
    let execution = claims.iter().find_map(|claim| {
        if let DeliveryClaim::Execute(lease) = claim {
            Some(lease)
        } else {
            None
        }
    });
    let busy = claims.iter().find_map(|claim| {
        if let DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis,
        } = claim
        {
            Some((evaluation_id, retry_at_unix_millis))
        } else {
            None
        }
    });
    let execution = execution.unwrap();
    let (evaluation_id, retry_at_unix_millis) = busy.unwrap();
    assert_eq!(evaluation_id, &execution.evaluation_id);
    assert_eq!(*retry_at_unix_millis, execution.expires_at_unix_millis);
}
