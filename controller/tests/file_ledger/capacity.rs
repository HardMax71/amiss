use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    ControllerClock, DeliveryClaim, DeliveryLedger, FileLedger, FileLedgerConfig, FileLedgerError,
    LeaseCompletion, ReplayWindow,
};
use tempfile::TempDir;

use super::support::{
    BOUNDED_ISSUED_AT, BOUNDED_KEEP_THROUGH, LEASE, TestClock, bounded_delivery, check_binding,
    config, delivery_with_id, executed, is_delivery_file, open_with_max, publication,
    replay_window, staged,
};

#[test]
fn capacity_rejects_new_records_without_blocking_existing_work() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let admitted = delivery_with_id("admitted", "41");
    let rejected = delivery_with_id("rejected", "42");
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    let lease = executed(ledger.claim(&admitted, &check_binding()).unwrap()).unwrap();

    assert!(matches!(
        ledger.claim(&rejected, &check_binding()),
        Err(FileLedgerError::Full)
    ));
    let frozen = staged(
        ledger
            .stage(&admitted, &lease, &publication(&admitted, &lease))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        ledger.complete(&admitted, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(matches!(
        ledger.claim(&admitted, &check_binding()).unwrap(),
        DeliveryClaim::Duplicate { .. }
    ));
    assert!(matches!(
        ledger.claim(&rejected, &check_binding()),
        Err(FileLedgerError::Full)
    ));
}

#[test]
fn pruning_a_bounded_completion_frees_capacity_for_a_new_identity() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(BOUNDED_ISSUED_AT));
    let bounded = bounded_delivery("bounded-capacity", "41");
    let next = delivery_with_id("next", "42");
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    let lease = executed(ledger.claim(&bounded, &check_binding()).unwrap()).unwrap();
    let frozen = staged(
        ledger
            .stage(&bounded, &lease, &publication(&bounded, &lease))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        ledger.complete(&bounded, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(matches!(
        ledger.claim(&next, &check_binding()),
        Err(FileLedgerError::Full)
    ));

    clock.set(BOUNDED_KEEP_THROUGH + 1);
    assert_eq!(ledger.cleanup().unwrap().removed_records, 1);
    assert!(matches!(
        ledger.claim(&next, &check_binding()).unwrap(),
        DeliveryClaim::Execute(_)
    ));
}

#[test]
fn immutable_root_limits_must_match_on_reopen() {
    let lease_directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    drop(open_with_max(lease_directory.path(), &clock, 1));
    let longer_lease = LEASE.checked_add(Duration::from_millis(1)).unwrap();
    let different_lease = FileLedgerConfig::new(longer_lease, 1, replay_window()).unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(lease_directory.path(), different_lease, clock_source),
        Err(FileLedgerError::Configuration)
    ));

    let maximum_directory = TempDir::new().unwrap();
    drop(open_with_max(maximum_directory.path(), &clock, 1));
    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(maximum_directory.path(), config(2), clock_source),
        Err(FileLedgerError::Configuration)
    ));

    let replay_directory = TempDir::new().unwrap();
    drop(open_with_max(replay_directory.path(), &clock, 1));
    let different_replay =
        ReplayWindow::new(Duration::from_secs(61), Duration::from_secs(10)).unwrap();
    let different_config = FileLedgerConfig::new(LEASE, 1, different_replay).unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock;
    assert!(matches!(
        FileLedger::open_with_clock(replay_directory.path(), different_config, clock_source),
        Err(FileLedgerError::Configuration)
    ));
}

#[test]
fn rejected_identities_create_only_a_fixed_number_of_lock_files() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let admitted = delivery_with_id("admitted", "1");
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    ledger.claim(&admitted, &check_binding()).unwrap();

    for number in 0..1_024 {
        let rejected = delivery_with_id(&format!("rejected-{number}"), &format!("{}", number + 2));
        assert!(matches!(
            ledger.claim(&rejected, &check_binding()),
            Err(FileLedgerError::Full)
        ));
    }

    let names = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    let row_locks = names
        .iter()
        .filter(|name| name.starts_with(".amiss-row-") && has_extension(name, "lock"))
        .count();
    assert!(row_locks <= 256);
    assert_eq!(
        names
            .iter()
            .filter(|name| is_delivery_file(name, ".state"))
            .count(),
        1
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| {
                has_extension(name, "lock")
                    && !name.starts_with(".amiss-row-")
                    && !matches!(
                        name.as_str(),
                        ".amiss-maintenance.lock" | ".amiss-admission.lock" | ".amiss-clock.lock"
                    )
            })
            .count(),
        0
    );
    assert!(names.len() <= 261);
}

#[test]
fn a_missing_root_record_cannot_be_recreated_over_existing_state() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    ledger
        .claim(&delivery_with_id("admitted", "42"), &check_binding())
        .unwrap();
    drop(ledger);
    fs::remove_file(directory.path().join(".amiss-root.state")).unwrap();

    let clock_source: Arc<dyn ControllerClock> = clock;
    assert!(matches!(
        FileLedger::open_with_clock(directory.path(), config(1), clock_source),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn a_bounded_delivery_from_another_replay_window_is_rejected() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(BOUNDED_ISSUED_AT));
    let delivery = bounded_delivery("bounded-window", "42");
    let replay = ReplayWindow::new(Duration::from_secs(61), Duration::from_secs(10)).unwrap();
    let config = FileLedgerConfig::new(LEASE, 1, replay).unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock;
    let mut ledger = FileLedger::open_with_clock(directory.path(), config, clock_source).unwrap();

    assert!(matches!(
        ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Configuration)
    ));
}

fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name).extension() == Some(OsStr::new(extension))
}
