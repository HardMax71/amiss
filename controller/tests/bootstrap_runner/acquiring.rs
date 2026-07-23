use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

struct CloneAcquisition<'a> {
    repository: &'a Path,
    action: &'a Path,
    paths: Arc<AcquiredPaths>,
}

impl Acquisition for CloneAcquisition<'_> {
    type Error = std::io::Error;

    fn acquire(
        &mut self,
        _request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        self.paths
            .0
            .lock()
            .unwrap()
            .extend([target.repository.to_path_buf(), target.action.to_path_buf()]);
        clone_repository(self.repository, target.repository, &target.cancelled)?;
        clone_repository(self.action, target.action, &target.cancelled)
    }
}

fn clone_repository(source: &Path, target: &Path, cancelled: &AtomicBool) -> std::io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(std::io::Error::other("acquisition cancelled"));
    }
    let source = path_arg(source);
    git(target, &["clone", "-q", &source, "."]).map(|_output| ())
}

struct RejectingAcquisition;

impl Acquisition for RejectingAcquisition {
    type Error = ();

    fn acquire(
        &mut self,
        _request: &RunRequest,
        _target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        Err(())
    }
}

struct WaitingAcquisition {
    observed: Arc<AtomicBool>,
}

impl Acquisition for WaitingAcquisition {
    type Error = ();

    fn acquire(
        &mut self,
        _request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        while !target.cancelled.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.observed.store(true, Ordering::Release);
        Err(())
    }
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
    let acquisition = CloneAcquisition {
        repository: harness.repository.root(),
        action: harness.action.root(),
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
    assert_eq!(heartbeat.calls, 2);
    assert!(paths.0.lock().unwrap().iter().all(|path| !path.exists()));
}

#[test]
fn acquisition_and_clock_defects_are_unavailable() {
    let harness = Harness::new("runner-pass", None);
    let mut rejected = acquiring_runner(&harness, RejectingAcquisition, Some(1_753_219_200_000));
    let mut heartbeat = Heartbeat::renewing();
    assert_eq!(
        rejected.run(&harness.request, &mut heartbeat),
        RunnerOutcome::Unavailable
    );

    let acquisition = CloneAcquisition {
        repository: harness.repository.root(),
        action: harness.action.root(),
        paths: Arc::new(AcquiredPaths::default()),
    };
    let mut no_time = acquiring_runner(&harness, acquisition, None);
    assert_eq!(
        no_time.run(&harness.request, &mut Heartbeat::renewing()),
        RunnerOutcome::Unavailable
    );
}

#[test]
fn heartbeat_loss_cancels_acquisition() {
    let harness = Harness::new("runner-pass", None);
    let observed = Arc::new(AtomicBool::new(false));
    let mut runner = acquiring_runner(
        &harness,
        WaitingAcquisition {
            observed: Arc::clone(&observed),
        },
        Some(1_753_219_200_000),
    );
    let mut heartbeat = Heartbeat::stopping_on(2);

    assert_eq!(
        runner.run(&harness.request, &mut heartbeat),
        RunnerOutcome::Unavailable
    );
    assert_eq!(heartbeat.calls, 2);
    assert!(observed.load(Ordering::Acquire));
}

#[test]
fn invalid_time_bounds_are_rejected() {
    let harness = Harness::new("runner-pass", None);
    let clock: Arc<dyn ControllerClock> = Arc::new(FixedClock(Some(1_753_219_200_000)));
    let build = |validity| {
        AcquiringRunner::new(
            RejectingAcquisition,
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
