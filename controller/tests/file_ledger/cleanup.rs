use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    AcceptedDelivery, DeliveryClaim, DeliveryLedger, FileLedger, FileLedgerCleanup,
    FileLedgerError, LeaseCompletion, StagedPublication,
};
use tempfile::TempDir;

use super::support::{
    BOUNDED_ISSUED_AT, BOUNDED_KEEP_THROUGH, FIXTURE_KEY, TestClock, bounded_delivery, delivery,
    executed, is_delivery_file, open, publication, staged,
};

#[test]
fn permanent_completion_survives_cleanup() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let finished = finish(&mut ledger, &delivery);
    let report_path = directory.path().join(format!("{FIXTURE_KEY}.report"));
    fs::write(&report_path, b"dead report").unwrap();

    clock.set(i64::MAX);
    assert_eq!(
        ledger.cleanup().unwrap(),
        FileLedgerCleanup {
            removed_records: 0,
            removed_reports: 1,
            removed_temporary: 0,
        }
    );
    assert!(!report_path.exists());
    assert_eq!(count_record_files(directory.path(), ".state"), 1);
    assert!(matches!(
        ledger.claim(&delivery).unwrap(),
        DeliveryClaim::Duplicate { evaluation_id }
            if evaluation_id == finished.evaluation_id
    ));
}

#[test]
fn bounded_completion_uses_an_inclusive_cutoff_and_rollback_cannot_reopen_it() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(BOUNDED_ISSUED_AT));
    let delivery = bounded_delivery("bounded-cutoff", "42");
    let mut ledger = open(directory.path(), &clock);
    let finished = finish(&mut ledger, &delivery);

    clock.set(BOUNDED_KEEP_THROUGH);
    assert_eq!(ledger.cleanup().unwrap(), FileLedgerCleanup::default());
    assert!(matches!(
        ledger.claim(&delivery).unwrap(),
        DeliveryClaim::Duplicate { evaluation_id }
            if evaluation_id == finished.evaluation_id
    ));

    clock.set(BOUNDED_KEEP_THROUGH + 1);
    assert_eq!(
        ledger.cleanup().unwrap(),
        FileLedgerCleanup {
            removed_records: 1,
            removed_reports: 0,
            removed_temporary: 0,
        }
    );
    assert_eq!(count_record_files(directory.path(), ".state"), 0);

    clock.set(BOUNDED_ISSUED_AT);
    assert!(matches!(
        ledger.claim(&delivery),
        Err(FileLedgerError::Expired)
    ));
    assert_eq!(
        ledger.complete(&delivery, &finished).unwrap(),
        LeaseCompletion::Lost
    );
}

#[test]
fn an_expired_unseen_delivery_stays_expired_after_clock_rollback() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(BOUNDED_ISSUED_AT));
    let delivery = bounded_delivery("bounded-unseen", "42");
    let mut ledger = open(directory.path(), &clock);

    clock.set(BOUNDED_KEEP_THROUGH + 1);
    assert!(matches!(
        ledger.claim(&delivery),
        Err(FileLedgerError::Expired)
    ));
    clock.set(BOUNDED_ISSUED_AT);
    drop(ledger);
    let mut ledger = open(directory.path(), &clock);
    assert!(matches!(
        ledger.claim(&delivery),
        Err(FileLedgerError::Expired)
    ));
    assert_eq!(count_record_files(directory.path(), ".state"), 0);
}

#[test]
fn expired_running_and_staged_work_is_never_pruned() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(BOUNDED_ISSUED_AT));
    let running = bounded_delivery("bounded-running", "41");
    let staged_delivery = bounded_delivery("bounded-staged", "42");
    let mut ledger = open(directory.path(), &clock);
    let _running_lease = executed(ledger.claim(&running).unwrap()).unwrap();
    let staged_lease = executed(ledger.claim(&staged_delivery).unwrap()).unwrap();
    let frozen = staged(
        ledger
            .stage(
                &staged_delivery,
                &staged_lease,
                &publication(&staged_delivery, &staged_lease),
            )
            .unwrap(),
    )
    .unwrap();

    clock.set(BOUNDED_KEEP_THROUGH + 1);
    assert_eq!(ledger.cleanup().unwrap(), FileLedgerCleanup::default());
    assert_eq!(count_record_files(directory.path(), ".state"), 2);
    assert_eq!(count_record_files(directory.path(), ".report"), 1);
    assert_eq!(
        ledger.claim(&staged_delivery).unwrap(),
        DeliveryClaim::Publish(frozen)
    );
}

#[test]
fn dead_atomic_write_directories_are_removed_only_in_the_known_shape() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let ledger = open(directory.path(), &clock);
    let empty = directory.path().join(".atomicwrite-empty");
    let with_file = directory.path().join(".atomicwrite-file");
    fs::create_dir(&empty).unwrap();
    fs::create_dir(&with_file).unwrap();
    fs::write(with_file.join("tmpfile.tmp"), b"partial").unwrap();

    assert_eq!(
        ledger.cleanup().unwrap(),
        FileLedgerCleanup {
            removed_records: 0,
            removed_reports: 0,
            removed_temporary: 2,
        }
    );
    assert!(!empty.exists());
    assert!(!with_file.exists());
}

#[test]
fn unknown_entries_and_malformed_temporary_directories_fail_closed() {
    let unknown_directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let unknown_ledger = open(unknown_directory.path(), &clock);
    fs::write(unknown_directory.path().join("foreign-file"), b"data").unwrap();
    assert!(matches!(
        unknown_ledger.cleanup(),
        Err(FileLedgerError::Corrupt)
    ));

    let temporary_directory = TempDir::new().unwrap();
    let temporary_ledger = open(temporary_directory.path(), &clock);
    let malformed = temporary_directory.path().join(".atomicwrite-malformed");
    fs::create_dir(&malformed).unwrap();
    fs::write(malformed.join("unexpected"), b"data").unwrap();
    assert!(matches!(
        temporary_ledger.cleanup(),
        Err(FileLedgerError::Corrupt)
    ));
    assert!(malformed.exists());
}

#[test]
fn corrupt_root_metadata_and_a_renamed_valid_record_fail_closed() {
    let metadata_directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let metadata_ledger = open(metadata_directory.path(), &clock);
    fs::write(
        metadata_directory.path().join(".amiss-root.state"),
        b"truncated",
    )
    .unwrap();
    assert!(matches!(
        metadata_ledger.cleanup(),
        Err(FileLedgerError::Corrupt)
    ));

    let record_directory = TempDir::new().unwrap();
    let mut record_ledger = open(record_directory.path(), &clock);
    record_ledger.claim(&delivery("42")).unwrap();
    let state = record_directory.path().join(format!("{FIXTURE_KEY}.state"));
    let renamed = record_directory
        .path()
        .join(format!("{}.state", "1".repeat(64)));
    fs::rename(state, renamed).unwrap();
    assert!(matches!(
        record_ledger.cleanup(),
        Err(FileLedgerError::Corrupt)
    ));
}

#[cfg(unix)]
#[test]
fn symlinks_in_the_record_namespace_fail_closed() {
    use std::os::unix::fs::symlink;

    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let ledger = open(directory.path(), &clock);
    symlink(
        directory.path().join(".amiss-root.state"),
        directory.path().join(format!("{FIXTURE_KEY}.state")),
    )
    .unwrap();

    assert!(matches!(ledger.cleanup(), Err(FileLedgerError::Corrupt)));
}

fn finish(ledger: &mut FileLedger, delivery: &AcceptedDelivery) -> StagedPublication {
    let lease = executed(ledger.claim(delivery).unwrap()).unwrap();
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
    frozen
}

fn count_record_files(root: &Path, suffix: &str) -> usize {
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_delivery_file(name, suffix))
        })
        .count()
}
