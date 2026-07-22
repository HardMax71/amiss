use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use amiss_controller::{DeliveryClaim, DeliveryLedger, FileLedgerError};
use tempfile::TempDir;

use super::support::{MAX_RECORDS, TestClock, check_binding, delivery_with_id, open_with_max};

const OWNER_TEST_NAME: &str =
    "process_locking::concurrent_first_claims_choose_one_owner_across_processes";
const CAPACITY_TEST_NAME: &str =
    "process_locking::concurrent_distinct_claims_enforce_capacity_across_processes";
const LEDGER_ROOT_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_ROOT";
const READY_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_READY";
const GATE_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_GATE";
const RESULT_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_RESULT";
const DELIVERY_ID_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_DELIVERY";
const MAX_RECORDS_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_MAX_RECORDS";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const EXECUTE: &[u8] = b"execute";
const BUSY: &[u8] = b"busy";
const FULL: &[u8] = b"full";

struct ChildRun {
    process: Child,
    ready_path: PathBuf,
    result_path: PathBuf,
    status: Option<ExitStatus>,
}

impl ChildRun {
    fn refresh(&mut self) {
        if self.status.is_none() {
            self.status = self.process.try_wait().unwrap();
        }
    }
}

impl Drop for ChildRun {
    fn drop(&mut self) {
        if self.status.is_none() {
            drop(self.process.kill());
        }
    }
}

#[test]
fn concurrent_first_claims_choose_one_owner_across_processes() {
    assert_race(
        OWNER_TEST_NAME,
        MAX_RECORDS,
        ["delivery-9", "delivery-9"],
        BUSY,
    );
}

#[test]
fn concurrent_distinct_claims_enforce_capacity_across_processes() {
    assert_race(
        CAPACITY_TEST_NAME,
        1,
        ["capacity-first", "capacity-second"],
        FULL,
    );
}

fn assert_race(test_name: &str, max_records: u64, delivery_ids: [&str; 2], other: &[u8]) {
    if env::var_os(LEDGER_ROOT_ENV).is_some() {
        run_child();
        return;
    }

    let outcomes = concurrent_outcomes(test_name, max_records, delivery_ids);
    assert_eq!(count(&outcomes, EXECUTE), 1);
    assert_eq!(count(&outcomes, other), 1);
}

fn concurrent_outcomes(test_name: &str, max_records: u64, delivery_ids: [&str; 2]) -> Vec<Vec<u8>> {
    let [first_delivery, second_delivery] = delivery_ids;
    let directory = TempDir::new().unwrap();
    let ledger_root = directory.path().join("ledger");
    fs::create_dir(&ledger_root).unwrap();
    let gate_path = directory.path().join("start");
    let mut children = [
        spawn_child(
            test_name,
            directory.path(),
            &ledger_root,
            &gate_path,
            "first",
            first_delivery,
            max_records,
        ),
        spawn_child(
            test_name,
            directory.path(),
            &ledger_root,
            &gate_path,
            "second",
            second_delivery,
            max_records,
        ),
    ];

    let ready = wait_until(PROCESS_TIMEOUT, || {
        children.iter().all(|child| child.ready_path.is_file())
    });
    if !ready {
        stop(&mut children);
    }
    assert!(ready, "child processes did not reach the start gate");

    fs::write(&gate_path, b"start").unwrap();
    let finished = wait_until(PROCESS_TIMEOUT, || {
        children.iter_mut().for_each(ChildRun::refresh);
        children.iter().all(|child| child.status.is_some())
    });
    if !finished {
        stop(&mut children);
    }
    assert!(
        finished,
        "child processes did not finish before the deadline"
    );
    assert!(
        children
            .iter()
            .all(|child| child.status.as_ref().is_some_and(ExitStatus::success)),
        "a child process failed"
    );

    children
        .iter()
        .map(|child| fs::read(&child.result_path).unwrap())
        .collect()
}

fn run_child() {
    let root = env_path(LEDGER_ROOT_ENV);
    let ready_path = env_path(READY_PATH_ENV);
    let gate_path = env_path(GATE_PATH_ENV);
    let result_path = env_path(RESULT_PATH_ENV);
    let delivery_id = env::var(DELIVERY_ID_ENV).unwrap();
    let max_records = env::var(MAX_RECORDS_ENV).unwrap().parse().unwrap();
    let clock = Arc::new(TestClock::new(1_000));
    let mut ledger = open_with_max(&root, &clock, max_records);

    fs::write(ready_path, b"ready").unwrap();
    assert!(
        wait_until(PROCESS_TIMEOUT, || gate_path.is_file()),
        "parent process did not open the start gate"
    );

    let claim = ledger.claim(&delivery_with_id(&delivery_id, "42"), &check_binding());
    let outcome = if matches!(&claim, Err(FileLedgerError::Full)) {
        Some(FULL)
    } else {
        match claim.ok() {
            Some(DeliveryClaim::Execute(_)) => Some(EXECUTE),
            Some(DeliveryClaim::Busy { .. }) => Some(BUSY),
            Some(
                DeliveryClaim::Publish(_)
                | DeliveryClaim::Duplicate { .. }
                | DeliveryClaim::BindingConflict,
            )
            | None => None,
        }
    };
    assert!(
        outcome.is_some(),
        "child received an unexpected claim outcome"
    );
    fs::write(result_path, outcome.unwrap()).unwrap();
}

fn spawn_child(
    test_name: &str,
    directory: &Path,
    ledger_root: &Path,
    gate_path: &Path,
    name: &str,
    delivery_id: &str,
    max_records: u64,
) -> ChildRun {
    let ready_path = directory.join(format!("{name}.ready"));
    let result_path = directory.join(format!("{name}.result"));
    let process = Command::new(env::current_exe().unwrap())
        .arg(test_name)
        .arg("--exact")
        .env(LEDGER_ROOT_ENV, ledger_root)
        .env(READY_PATH_ENV, &ready_path)
        .env(GATE_PATH_ENV, gate_path)
        .env(RESULT_PATH_ENV, &result_path)
        .env(DELIVERY_ID_ENV, delivery_id)
        .env(MAX_RECORDS_ENV, max_records.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    ChildRun {
        process,
        ready_path,
        result_path,
        status: None,
    }
}

fn count(outcomes: &[Vec<u8>], expected: &[u8]) -> usize {
    outcomes
        .iter()
        .filter(|outcome| outcome.as_slice() == expected)
        .count()
}

fn env_path(name: &str) -> PathBuf {
    env::var_os(name).map(PathBuf::from).unwrap()
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now().checked_add(timeout).unwrap();
    while !predicate() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL_INTERVAL);
    }
    true
}

fn stop(children: &mut [ChildRun]) {
    children
        .iter_mut()
        .filter(|child| child.status.is_none())
        .for_each(|child| drop(child.process.kill()));
    let stopped = wait_until(STOP_TIMEOUT, || {
        children.iter_mut().for_each(ChildRun::refresh);
        children.iter().all(|child| child.status.is_some())
    });
    assert!(stopped, "child processes did not stop after termination");
}
