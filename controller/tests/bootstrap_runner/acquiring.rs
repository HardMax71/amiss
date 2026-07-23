use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use amiss_controller::{AcquiringRunner, Acquisition, AcquisitionTarget, ControllerClock, Runner};
use amiss_fixtures::path_arg;

use super::*;

struct FixedClock(Option<i64>);

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        self.0
    }
}

#[derive(Default)]
struct AcquiredPaths(Mutex<Vec<PathBuf>>);

enum AcquisitionFixture {
    Clone {
        repository: PathBuf,
        action: PathBuf,
        paths: Arc<AcquiredPaths>,
    },
    Reject,
    IgnoreCancellation {
        started: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
        finished: mpsc::SyncSender<()>,
    },
}

impl Acquisition for AcquisitionFixture {
    type Error = std::io::Error;

    fn acquire(
        &mut self,
        _request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        match self {
            Self::Clone {
                repository,
                action,
                paths,
            } => {
                paths
                    .0
                    .lock()
                    .unwrap()
                    .extend([target.repository.to_path_buf(), target.action.to_path_buf()]);
                clone_repository(repository, target.repository, &target.cancelled)?;
                clone_repository(action, target.action, &target.cancelled)
            }
            Self::Reject => Err(std::io::Error::other("acquisition rejected")),
            Self::IgnoreCancellation {
                started,
                release,
                finished,
            } => {
                let _ignored = started.send(());
                let _ignored = release.recv();
                let _ignored = finished.send(());
                Err(std::io::Error::other("acquisition cancelled"))
            }
        }
    }
}

fn clone_repository(source: &Path, target: &Path, cancelled: &AtomicBool) -> std::io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(std::io::Error::other("acquisition cancelled"));
    }
    let source = path_arg(source);
    git(target, &["clone", "-q", &source, "."]).map(|_output| ())
}

fn acquiring_runner<A: Acquisition>(
    harness: &Harness,
    acquisition: A,
    clock: Option<i64>,
) -> AcquiringRunner<A> {
    AcquiringRunner::new(
        acquisition,
        harness.executable.clone(),
        harness.scratch.path().to_path_buf(),
        Duration::from_secs(2),
        Duration::from_mins(5),
        Arc::new(FixedClock(clock)),
    )
    .unwrap()
}

#[test]
fn uses_private_exact_roots_and_controller_time() {
    let harness = Harness::new("runner-pass", None);
    let paths = Arc::new(AcquiredPaths::default());
    let acquisition = AcquisitionFixture::Clone {
        repository: harness.repository.root().to_path_buf(),
        action: harness.action.root().to_path_buf(),
        paths: Arc::clone(&paths),
    };
    let mut runner = acquiring_runner(&harness, acquisition, Some(1_753_219_200_000));
    let mut heartbeat = Heartbeat::renewing();

    assert_eq!(
        runner.run(&harness.request, &mut heartbeat),
        RunnerOutcome::Complete {
            identity: Box::new(harness.request.run.clone()),
            evaluation: Evaluation::Pass,
            report: PASS_REPORT.to_vec(),
        }
    );
    assert!(heartbeat.calls >= 2);
    assert!(paths.0.lock().unwrap().iter().all(|path| !path.exists()));
}

#[test]
fn acquisition_and_clock_defects_are_unavailable() {
    let harness = Harness::new("runner-pass", None);
    let mut rejected = acquiring_runner(
        &harness,
        AcquisitionFixture::Reject,
        Some(1_753_219_200_000),
    );
    let mut heartbeat = Heartbeat::renewing();
    assert_eq!(
        rejected.run(&harness.request, &mut heartbeat),
        RunnerOutcome::Unavailable
    );

    let acquisition = AcquisitionFixture::Clone {
        repository: harness.repository.root().to_path_buf(),
        action: harness.action.root().to_path_buf(),
        paths: Arc::new(AcquiredPaths::default()),
    };
    let mut no_time = acquiring_runner(&harness, acquisition, None);
    assert_eq!(
        no_time.run(&harness.request, &mut Heartbeat::renewing()),
        RunnerOutcome::Unavailable
    );
}

#[test]
fn heartbeat_loss_returns_before_uncooperative_acquisition() {
    let harness = Harness::new("runner-pass", None);
    let (started_sender, started) = mpsc::sync_channel(1);
    let (release, release_receiver) = mpsc::sync_channel(0);
    let (finished_sender, finished) = mpsc::sync_channel(1);
    let runner = acquiring_runner(
        &harness,
        AcquisitionFixture::IgnoreCancellation {
            started: started_sender,
            release: release_receiver,
            finished: finished_sender,
        },
        Some(1_753_219_200_000),
    );
    let request = harness.request.clone();
    let (outcome_sender, outcome) = mpsc::sync_channel(1);
    let runner_thread = std::thread::spawn(move || {
        let mut runner = runner;
        let mut heartbeat = Heartbeat::stopping_on(2);
        let result = runner.run(&request, &mut heartbeat);
        let _ignored = outcome_sender.send((result, heartbeat.calls));
    });

    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let returned_while_acquisition_was_blocked = outcome.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(2)).unwrap();
    runner_thread.join().unwrap();

    assert_eq!(
        returned_while_acquisition_was_blocked.unwrap(),
        (RunnerOutcome::Unavailable, 2)
    );
}

#[test]
fn invalid_time_bounds_are_rejected() {
    let harness = Harness::new("runner-pass", None);
    let clock: Arc<dyn ControllerClock> = Arc::new(FixedClock(Some(1_753_219_200_000)));
    let build = |validity| {
        AcquiringRunner::new(
            AcquisitionFixture::Reject,
            harness.executable.clone(),
            harness.scratch.path().to_path_buf(),
            Duration::from_secs(2),
            validity,
            Arc::clone(&clock),
        )
    };

    assert!(build(Duration::ZERO).is_none());
    assert!(build(Duration::from_millis(1_500)).is_none());
    assert!(build(Duration::from_secs(601)).is_none());
}
