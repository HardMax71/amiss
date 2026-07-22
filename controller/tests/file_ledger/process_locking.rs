use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use amiss_controller::{DeliveryClaim, DeliveryLedger};
use tempfile::TempDir;

use super::support::{TestClock, delivery, open};

const TEST_NAME: &str =
    "process_locking::concurrent_first_claims_choose_one_owner_across_processes";
const LEDGER_ROOT_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_ROOT";
const READY_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_READY";
const GATE_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_GATE";
const RESULT_PATH_ENV: &str = "AMISS_TEST_FILE_LEDGER_PROCESS_RESULT";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const EXECUTE: &[u8] = b"execute";
const BUSY: &[u8] = b"busy";

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
    if env::var_os(LEDGER_ROOT_ENV).is_some() {
        run_child();
        return;
    }

    let directory = TempDir::new().unwrap();
    let ledger_root = directory.path().join("ledger");
    fs::create_dir(&ledger_root).unwrap();
    let gate_path = directory.path().join("start");
    let mut children = [
        spawn_child(directory.path(), &ledger_root, &gate_path, "first"),
        spawn_child(directory.path(), &ledger_root, &gate_path, "second"),
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

    let outcomes = children
        .iter()
        .map(|child| fs::read(&child.result_path).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| outcome.as_slice() == EXECUTE)
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| outcome.as_slice() == BUSY)
            .count(),
        1
    );
}

fn run_child() {
    let root = env_path(LEDGER_ROOT_ENV);
    let ready_path = env_path(READY_PATH_ENV);
    let gate_path = env_path(GATE_PATH_ENV);
    let result_path = env_path(RESULT_PATH_ENV);
    let clock = Arc::new(TestClock::new(1_000));
    let mut ledger = open(&root, &clock);

    fs::write(ready_path, b"ready").unwrap();
    assert!(
        wait_until(PROCESS_TIMEOUT, || gate_path.is_file()),
        "parent process did not open the start gate"
    );

    let outcome = match ledger.claim(&delivery("42")).unwrap() {
        DeliveryClaim::Execute(_) => Some(EXECUTE),
        DeliveryClaim::Busy { .. } => Some(BUSY),
        DeliveryClaim::Publish(_)
        | DeliveryClaim::Duplicate { .. }
        | DeliveryClaim::BindingConflict => None,
    };
    assert!(
        outcome.is_some(),
        "child received an unexpected claim outcome"
    );
    fs::write(result_path, outcome.unwrap()).unwrap();
}

fn spawn_child(directory: &Path, ledger_root: &Path, gate_path: &Path, name: &str) -> ChildRun {
    let ready_path = directory.join(format!("{name}.ready"));
    let result_path = directory.join(format!("{name}.result"));
    let process = Command::new(env::current_exe().unwrap())
        .arg(TEST_NAME)
        .arg("--exact")
        .env(LEDGER_ROOT_ENV, ledger_root)
        .env(READY_PATH_ENV, &ready_path)
        .env(GATE_PATH_ENV, gate_path)
        .env(RESULT_PATH_ENV, &result_path)
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
