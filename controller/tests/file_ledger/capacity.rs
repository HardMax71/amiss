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
    BOUNDED_ISSUED_AT, BOUNDED_KEEP_THROUGH, LEASE, TestClock, bounded_delivery,
    bounded_delivery_at, check_binding, config, delivery_with_id, downgrade_root_metadata,
    executed, is_delivery_file, ledger_file, open_with_max, publication, replay_window, staged,
    write_capacity,
};

#[test]
fn capacity_rejects_new_records_without_blocking_existing_work() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
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
fn explicit_cleanup_frees_an_ended_bounded_slot() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(BOUNDED_ISSUED_AT);
    let bounded = bounded_delivery("bounded-capacity", "41");
    let next_issued_at = BOUNDED_KEEP_THROUGH + 1_000;
    let next = bounded_delivery_at("next", "42", next_issued_at);
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

    clock.set(next_issued_at);
    assert_eq!(ledger.cleanup().unwrap().removed_records, 1);
    assert!(matches!(
        ledger.claim(&next, &check_binding()).unwrap(),
        DeliveryClaim::Execute(_)
    ));
}

#[test]
fn cleanup_frees_exactly_the_removed_slots_after_reopen() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(BOUNDED_ISSUED_AT);
    let first = bounded_delivery("bounded-first", "41");
    let second = bounded_delivery("bounded-second", "42");
    let running = bounded_delivery("bounded-running", "43");
    let mut ledger = open_with_max(directory.path(), &clock, 3);

    for delivery in [&first, &second] {
        let lease = executed(ledger.claim(delivery, &check_binding()).unwrap()).unwrap();
        let frozen = staged(
            ledger
                .stage(delivery, &lease, &publication(delivery, &lease))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            ledger.complete(delivery, &frozen).unwrap(),
            LeaseCompletion::Completed
        );
    }
    ledger.claim(&running, &check_binding()).unwrap();

    clock.set(BOUNDED_KEEP_THROUGH + 1);
    assert_eq!(ledger.cleanup().unwrap().removed_records, 2);
    drop(ledger);

    let mut reopened = open_with_max(directory.path(), &clock, 3);
    for (delivery_id, change_id) in [("replacement-first", "44"), ("replacement-second", "45")] {
        assert!(matches!(
            reopened
                .claim(&delivery_with_id(delivery_id, change_id), &check_binding())
                .unwrap(),
            DeliveryClaim::Execute(_)
        ));
    }
    assert!(matches!(
        reopened.claim(
            &delivery_with_id("replacement-third", "46"),
            &check_binding()
        ),
        Err(FileLedgerError::Full)
    ));
}

#[test]
fn immutable_root_limits_must_match_on_reopen() {
    let lease_directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
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
    let clock = TestClock::at(1_000);
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
    assert!(names.len() <= 262);
}

#[test]
fn a_missing_root_record_cannot_be_recreated_over_existing_state() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
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
fn a_v09_root_migrates_without_losing_its_replay_marker() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let delivery = delivery_with_id("migrated", "42");
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let frozen = staged(
        ledger
            .stage(&delivery, &lease, &publication(&delivery, &lease))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        ledger.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    drop(ledger);
    downgrade_root_metadata(directory.path());

    let mut migrated = open_with_max(directory.path(), &clock, 1);
    assert!(matches!(
        migrated.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Duplicate { evaluation_id } if evaluation_id == frozen.evaluation_id
    ));
}

#[test]
fn missing_or_corrupt_capacity_and_a_missing_record_fail_closed() {
    let missing_capacity = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let mut ledger = open_with_max(missing_capacity.path(), &clock, 1);
    ledger
        .claim(&delivery_with_id("capacity", "41"), &check_binding())
        .unwrap();
    drop(ledger);
    fs::remove_file(missing_capacity.path().join(".amiss-capacity.state")).unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(missing_capacity.path(), config(1), clock_source),
        Err(FileLedgerError::Corrupt)
    ));

    let corrupt_capacity = TempDir::new().unwrap();
    drop(open_with_max(corrupt_capacity.path(), &clock, 1));
    fs::write(
        corrupt_capacity.path().join(".amiss-capacity.state"),
        b"truncated",
    )
    .unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock.clone();
    assert!(matches!(
        FileLedger::open_with_clock(corrupt_capacity.path(), config(1), clock_source),
        Err(FileLedgerError::Corrupt)
    ));

    let missing_record = TempDir::new().unwrap();
    let mut ledger = open_with_max(missing_record.path(), &clock, 1);
    ledger
        .claim(&delivery_with_id("record", "42"), &check_binding())
        .unwrap();
    drop(ledger);
    fs::remove_file(ledger_file(missing_record.path(), ".state").unwrap()).unwrap();
    let clock_source: Arc<dyn ControllerClock> = clock;
    assert!(matches!(
        FileLedger::open_with_clock(missing_record.path(), config(1), clock_source),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn interrupted_capacity_updates_recover_from_the_exact_pending_path() {
    let absent = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    drop(open_with_max(absent.path(), &clock, 2));
    write_capacity(absent.path(), 2, 1, Some(&"f".repeat(64)), false);
    let mut recovered = open_with_max(absent.path(), &clock, 2);
    assert!(matches!(
        recovered
            .claim(&delivery_with_id("after-absent", "41"), &check_binding())
            .unwrap(),
        DeliveryClaim::Execute(_)
    ));

    let present = TempDir::new().unwrap();
    let delivery = delivery_with_id("delivery-9", "42");
    let mut ledger = open_with_max(present.path(), &clock, 2);
    ledger.claim(&delivery, &check_binding()).unwrap();
    drop(ledger);
    write_capacity(
        present.path(),
        2,
        1,
        Some(super::support::FIXTURE_KEY),
        false,
    );
    let mut recovered = open_with_max(present.path(), &clock, 2);
    assert!(matches!(
        recovered.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Busy { .. }
    ));
}

#[test]
fn a_pending_key_is_settled_without_reopening_the_root() {
    let absent = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let mut ledger = open_with_max(absent.path(), &clock, 2);
    write_capacity(absent.path(), 2, 1, Some(&"e".repeat(64)), false);
    assert!(matches!(
        ledger
            .claim(&delivery_with_id("after-absent", "41"), &check_binding())
            .unwrap(),
        DeliveryClaim::Execute(_)
    ));

    let present = TempDir::new().unwrap();
    let mut ledger = open_with_max(present.path(), &clock, 2);
    ledger
        .claim(&delivery_with_id("delivery-9", "42"), &check_binding())
        .unwrap();
    write_capacity(
        present.path(),
        2,
        1,
        Some(super::support::FIXTURE_KEY),
        false,
    );
    assert!(matches!(
        ledger
            .claim(&delivery_with_id("second", "43"), &check_binding())
            .unwrap(),
        DeliveryClaim::Execute(_)
    ));
}

#[cfg(unix)]
#[test]
fn an_unreadable_pending_state_file_is_an_error_not_an_absence() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let mut ledger = open_with_max(directory.path(), &clock, 2);
    ledger
        .claim(&delivery_with_id("delivery-9", "42"), &check_binding())
        .unwrap();
    let state = ledger_file(directory.path(), ".state").unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o000)).unwrap();
    write_capacity(
        directory.path(),
        2,
        1,
        Some(super::support::FIXTURE_KEY),
        false,
    );

    assert!(matches!(
        ledger.claim(&delivery_with_id("second", "43"), &check_binding()),
        Err(FileLedgerError::Io(_))
    ));
}

#[test]
fn an_invalid_pending_key_is_corrupt_before_any_recovery() {
    let sixty_four_but_not_hex = "Z".repeat(64);
    for pending in ["ab", sixty_four_but_not_hex.as_str()] {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        let mut ledger = open_with_max(directory.path(), &clock, 2);
        write_capacity(directory.path(), 2, 1, Some(pending), false);
        assert!(matches!(
            ledger.claim(&delivery_with_id("after-invalid", "41"), &check_binding()),
            Err(FileLedgerError::Corrupt)
        ));
    }
}

#[test]
fn interrupted_batch_cleanup_reconciles_only_with_its_marker() {
    let clock = TestClock::at(1_000);
    let unchanged = TempDir::new().unwrap();
    let delivery = delivery_with_id("kept-by-cleanup", "40");
    let mut ledger = open_with_max(unchanged.path(), &clock, 2);
    ledger.claim(&delivery, &check_binding()).unwrap();
    drop(ledger);
    write_capacity(unchanged.path(), 2, 1, None, true);
    let mut recovered = open_with_max(unchanged.path(), &clock, 2);
    assert!(matches!(
        recovered.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Busy { .. }
    ));

    let directory = TempDir::new().unwrap();
    let mut ledger = open_with_max(directory.path(), &clock, 2);
    ledger
        .claim(
            &delivery_with_id("removed-by-cleanup", "41"),
            &check_binding(),
        )
        .unwrap();
    drop(ledger);

    write_capacity(directory.path(), 2, 1, None, true);
    fs::remove_file(ledger_file(directory.path(), ".state").unwrap()).unwrap();

    let mut recovered = open_with_max(directory.path(), &clock, 2);
    assert!(matches!(
        recovered
            .claim(&delivery_with_id("after-cleanup", "42"), &check_binding())
            .unwrap(),
        DeliveryClaim::Execute(_)
    ));
}

#[test]
fn a_wrong_capacity_limit_is_rejected_without_settling_it() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(1_000);
    let mut ledger = open_with_max(directory.path(), &clock, 1);
    write_capacity(directory.path(), 2, 1, Some(&"f".repeat(64)), false);
    let path = directory.path().join(".amiss-capacity.state");
    let before = fs::read(&path).unwrap();

    assert!(matches!(
        ledger.claim(&delivery_with_id("wrong-limit", "42"), &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn a_bounded_delivery_from_another_replay_window_is_rejected() {
    let directory = TempDir::new().unwrap();
    let clock = TestClock::at(BOUNDED_ISSUED_AT);
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

/// The capacity record holds one shape at a time, and every combination the
/// state machine cannot produce is refused when the root reopens.
#[test]
fn an_impossible_capacity_shape_fails_closed() {
    let key = "f".repeat(64);
    let rows: [(&str, u64, u64, Option<&str>, bool); 6] = [
        ("no records at all", 0, 0, None, false),
        ("more records than the maximum", 2, 3, None, false),
        ("a pending key over an empty root", 2, 0, Some(&key), false),
        ("a cleanup with a pending key", 2, 1, Some(&key), true),
        ("a cleanup over an empty root", 2, 0, None, true),
        ("a pending key off the wire", 2, 1, Some("not-a-key"), false),
    ];
    for (reason, maximum, records, pending, cleanup) in rows {
        let directory = TempDir::new().unwrap();
        let clock = TestClock::at(1_000);
        drop(open_with_max(directory.path(), &clock, 2));
        write_capacity(directory.path(), maximum, records, pending, cleanup);
        let clock_source: Arc<dyn ControllerClock> = clock.clone();
        assert!(
            matches!(
                FileLedger::open_with_clock(directory.path(), config(2), clock_source),
                Err(FileLedgerError::Corrupt)
            ),
            "{reason}"
        );
    }
}
