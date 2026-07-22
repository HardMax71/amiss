use std::fs;
use std::sync::Arc;

use amiss_controller::{
    ControllerClock, DeliveryClaim, DeliveryLedger, FileLedger, FileLedgerError, LeaseRenewal,
    StageOutcome,
};
use tempfile::TempDir;

use super::support::{MAX_RECORDS, TestClock, config, delivery, executed, open, publication};

#[test]
fn a_live_claim_resumes_for_its_owner_and_is_busy_for_another() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut first_owner = open(directory.path(), &clock);
    let first = executed(first_owner.claim(&delivery).unwrap()).unwrap();

    assert_eq!(
        executed(first_owner.claim(&delivery).unwrap()),
        Some(first.clone())
    );

    let mut second_owner = open(directory.path(), &clock);
    assert!(matches!(
        second_owner.claim(&delivery).unwrap(),
        DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis
        } if evaluation_id == first.evaluation_id
            && retry_at_unix_millis == first.expires_at_unix_millis
    ));
}

#[test]
fn the_record_root_must_already_be_a_directory() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let missing = directory.path().join("missing");
    let clock_source: Arc<dyn ControllerClock> = clock.clone();

    assert!(matches!(
        FileLedger::open_with_clock(&missing, config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound
    ));

    let file = directory.path().join("record-file");
    fs::write(&file, b"not a directory").unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock;
    assert!(matches!(
        FileLedger::open_with_clock(file, config(MAX_RECORDS), clock_source),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn expiry_reclaims_the_same_evaluation_with_a_higher_fence() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut first_owner = open(directory.path(), &clock);
    let mut second_owner = open(directory.path(), &clock);
    let first = executed(first_owner.claim(&delivery).unwrap()).unwrap();

    clock.set(first.expires_at_unix_millis);
    let reclaimed = executed(second_owner.claim(&delivery).unwrap()).unwrap();

    assert_eq!(reclaimed.evaluation_id, first.evaluation_id);
    assert_eq!(reclaimed.fence.get(), first.fence.get() + 1);
    assert_eq!(reclaimed.expires_at_unix_millis, 1_200);
    assert_eq!(
        first_owner.renew(&delivery, &first).unwrap(),
        LeaseRenewal::Lost
    );
    assert_eq!(
        first_owner
            .stage(&delivery, &first, &publication(&delivery, &first))
            .unwrap(),
        StageOutcome::Lost
    );
}

#[test]
fn renewal_advances_the_deadline_and_rejects_stale_or_rebound_claims() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let rebound = delivery("43");
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let first = executed(ledger.claim(&delivery).unwrap()).unwrap();

    clock.set(1_050);
    let renewed = match ledger.renew(&delivery, &first).unwrap() {
        LeaseRenewal::Renewed(lease) => Some(lease),
        LeaseRenewal::Lost => None,
    }
    .unwrap();

    assert_eq!(renewed.evaluation_id, first.evaluation_id);
    assert_eq!(renewed.fence, first.fence);
    assert_eq!(renewed.expires_at_unix_millis, 1_150);
    assert_eq!(ledger.renew(&delivery, &first).unwrap(), LeaseRenewal::Lost);
    assert_eq!(
        ledger.claim(&rebound).unwrap(),
        DeliveryClaim::BindingConflict
    );
}

#[test]
fn clock_rollback_does_not_shorten_a_persisted_lease() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut owner = open(directory.path(), &clock);
    let first = executed(owner.claim(&delivery).unwrap()).unwrap();

    clock.set(1_050);
    let renewed = match owner.renew(&delivery, &first).unwrap() {
        LeaseRenewal::Renewed(lease) => Some(lease),
        LeaseRenewal::Lost => None,
    }
    .unwrap();
    assert_eq!(renewed.expires_at_unix_millis, 1_150);

    clock.set(900);
    assert_eq!(
        owner.renew(&delivery, &renewed).unwrap(),
        LeaseRenewal::Renewed(renewed.clone())
    );
    let mut other_owner = open(directory.path(), &clock);
    assert!(matches!(
        other_owner.claim(&delivery).unwrap(),
        DeliveryClaim::Busy {
            evaluation_id,
            retry_at_unix_millis: 1_150
        } if evaluation_id == renewed.evaluation_id
    ));
}
