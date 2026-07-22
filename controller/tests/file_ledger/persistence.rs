use std::fs;
use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    DeliveryClaim, DeliveryLedger, FileLedgerError, LeaseCompletion, StageOutcome,
};
use amiss_wire::digest::hb;
use amiss_wire::report::MACHINE_JSON_BYTES;
use serde_json::{Map, Value};
use tempfile::TempDir;

use super::support::{
    TestClock, check_binding, delivery, executed, ledger_file, open, publication, staged,
};

#[test]
fn staged_bytes_survive_reopen_and_completion_is_repeat_safe() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let publication = publication(&delivery, &lease);
    let frozen = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();

    assert_eq!(
        ledger.stage(&delivery, &lease, &publication).unwrap(),
        StageOutcome::Staged(frozen.clone())
    );
    drop(ledger);

    let mut reopened = open(directory.path(), &clock);
    assert_eq!(
        reopened.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Publish(frozen.clone())
    );
    let report_path = ledger_file(directory.path(), ".report").unwrap();
    assert_eq!(
        reopened.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(!report_path.exists());
    fs::write(&report_path, b"orphaned report").unwrap();
    assert_eq!(
        reopened.complete(&delivery, &frozen).unwrap(),
        LeaseCompletion::Completed
    );
    assert!(!report_path.exists());
    drop(reopened);

    let mut after_restart = open(directory.path(), &clock);
    assert_eq!(
        after_restart.claim(&delivery, &check_binding()).unwrap(),
        DeliveryClaim::Duplicate {
            evaluation_id: lease.evaluation_id
        }
    );
}

#[test]
fn corrupt_state_or_report_fails_closed() {
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let state_directory = TempDir::new().unwrap();
    let mut state_ledger = open(state_directory.path(), &clock);
    state_ledger.claim(&delivery, &check_binding()).unwrap();
    fs::write(
        ledger_file(state_directory.path(), ".state").unwrap(),
        b"truncated",
    )
    .unwrap();
    assert!(matches!(
        state_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let report_directory = TempDir::new().unwrap();
    let mut report_ledger = open(report_directory.path(), &clock);
    let lease = executed(report_ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let publication = publication(&delivery, &lease);
    report_ledger
        .stage(&delivery, &lease, &publication)
        .unwrap();
    fs::write(
        ledger_file(report_directory.path(), ".report").unwrap(),
        b"tampered",
    )
    .unwrap();
    assert!(matches!(
        report_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn a_missing_staged_report_is_corrupt() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    ledger
        .stage(&delivery, &lease, &publication(&delivery, &lease))
        .unwrap();
    fs::remove_file(ledger_file(directory.path(), ".report").unwrap()).unwrap();

    assert!(matches!(
        ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn oversized_conflicting_completion_is_lost() {
    let directory = TempDir::new().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let mut ledger = open(directory.path(), &clock);
    let lease = executed(ledger.claim(&delivery, &check_binding()).unwrap()).unwrap();
    let publication = publication(&delivery, &lease);
    let mut conflicting = staged(ledger.stage(&delivery, &lease, &publication).unwrap()).unwrap();
    let oversized = usize::try_from(MACHINE_JSON_BYTES).unwrap() + 1;
    conflicting.publication.report = Some(vec![0; oversized]);

    assert_eq!(
        ledger.complete(&delivery, &conflicting).unwrap(),
        LeaseCompletion::Lost
    );
}

#[test]
fn impossible_but_checksummed_states_fail_closed() {
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");
    let expiry_directory = TempDir::new().unwrap();
    let mut expiry_ledger = open(expiry_directory.path(), &clock);
    expiry_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(expiry_directory.path(), |record| {
        let last_seen = record
            .get("last_seen_unix_millis")
            .and_then(Value::as_i64)
            .unwrap();
        record
            .get_mut("state")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("expires_at_unix_millis".to_owned(), last_seen.into());
    });
    assert!(matches!(
        expiry_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let fence_directory = TempDir::new().unwrap();
    let mut fence_ledger = open(fence_directory.path(), &clock);
    fence_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(fence_directory.path(), |record| {
        let generation = record.get("generation").and_then(Value::as_u64).unwrap();
        record
            .get_mut("state")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("fence".to_owned(), (generation + 1).into());
    });
    assert!(matches!(
        fence_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

#[test]
fn malformed_record_and_publication_check_bindings_fail_closed() {
    let clock = Arc::new(TestClock::new(1_000));
    let delivery = delivery("42");

    let record_directory = TempDir::new().unwrap();
    let mut record_ledger = open(record_directory.path(), &clock);
    record_ledger.claim(&delivery, &check_binding()).unwrap();
    rewrite_state(record_directory.path(), |record| {
        *record
            .get_mut("check")
            .and_then(Value::as_object_mut)
            .and_then(|check| check.get_mut("required_status_name"))
            .unwrap() = " invalid".into();
    });
    assert!(matches!(
        record_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));

    let publication_directory = TempDir::new().unwrap();
    let mut publication_ledger = open(publication_directory.path(), &clock);
    let lease = executed(
        publication_ledger
            .claim(&delivery, &check_binding())
            .unwrap(),
    )
    .unwrap();
    publication_ledger
        .stage(&delivery, &lease, &publication(&delivery, &lease))
        .unwrap();
    rewrite_state(publication_directory.path(), |record| {
        *record
            .get_mut("state")
            .and_then(Value::as_object_mut)
            .and_then(|state| state.get_mut("publication"))
            .and_then(Value::as_object_mut)
            .and_then(|publication| publication.get_mut("check"))
            .and_then(Value::as_object_mut)
            .and_then(|check| check.get_mut("required_status_name"))
            .unwrap() = "invalid ".into();
    });
    assert!(matches!(
        publication_ledger.claim(&delivery, &check_binding()),
        Err(FileLedgerError::Corrupt)
    ));
}

fn rewrite_state(root: &Path, change: impl FnOnce(&mut Map<String, Value>)) {
    const MAGIC: &[u8] = b"AMISS-DELIVERY-RECORD";
    const VERSION: u8 = 1;
    const DIGEST_BYTES: usize = 32;
    const DOMAIN: &str = "amiss/controller-file-record-v1";

    let path = ledger_file(root, ".state").unwrap();
    let frame = fs::read(&path).unwrap();
    let header_bytes = MAGIC.len() + 1 + size_of::<u64>() + DIGEST_BYTES;
    let payload = frame.get(header_bytes..).unwrap();
    let mut value: Value = serde_json::from_slice(payload).unwrap();
    change(value.as_object_mut().unwrap());
    let payload = serde_json::to_vec(&value).unwrap();
    let payload_length = u64::try_from(payload.len()).unwrap();
    let mut frame = Vec::with_capacity(header_bytes + payload.len());
    frame.extend_from_slice(MAGIC);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(hb(DOMAIN, &payload).as_bytes());
    frame.extend_from_slice(&payload);
    fs::write(path, frame).unwrap();
}
