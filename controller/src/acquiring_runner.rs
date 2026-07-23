use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use amiss_wire::controls::STATEMENT_TTL_MAX_SECONDS;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::WATCHDOG_MILLISECONDS;

use crate::bootstrap_runner::renewal_wait;
use crate::{
    BootstrapRun, ControllerClock, RunHeartbeat, RunRequest, Runner, RunnerOutcome, run_bootstrap,
};

pub struct AcquisitionTarget<'a> {
    pub repository: &'a Path,
    pub action: &'a Path,
    pub cancelled: Arc<AtomicBool>,
}

pub trait Acquisition: Send {
    type Error;

    /// Implementations bound their own I/O and stop promptly after loading
    /// cancellation with [`Ordering::Acquire`].
    ///
    /// # Errors
    ///
    /// The exact repository or action objects could not be acquired.
    fn acquire(
        &mut self,
        request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error>;
}

pub struct AcquiringRunner<A> {
    acquisition: Arc<Mutex<Option<A>>>,
    executable: PathBuf,
    scratch: PathBuf,
    wall_timeout: Duration,
    validity_seconds: i64,
    clock: Arc<dyn ControllerClock>,
}

impl<A> AcquiringRunner<A> {
    #[must_use]
    pub fn new(
        acquisition: A,
        executable: PathBuf,
        scratch: PathBuf,
        wall_timeout: Duration,
        validity: Duration,
        clock: Arc<dyn ControllerClock>,
    ) -> Option<Self> {
        let validity_seconds = i64::try_from(validity.as_secs()).ok()?;
        let valid_wall_timeout = wall_timeout > Duration::ZERO
            && wall_timeout <= Duration::from_millis(WATCHDOG_MILLISECONDS);
        let valid_validity = validity.subsec_nanos() == 0
            && (1..=STATEMENT_TTL_MAX_SECONDS).contains(&validity_seconds);
        (valid_wall_timeout && valid_validity).then_some(Self {
            acquisition: Arc::new(Mutex::new(Some(acquisition))),
            executable,
            scratch,
            wall_timeout,
            validity_seconds,
            clock,
        })
    }
}

impl<A: Acquisition + 'static> Runner for AcquiringRunner<A> {
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        let Ok(acquired) = AcquiredRun::new(&self.scratch) else {
            return RunnerOutcome::Unavailable;
        };
        let Some(renew_after) = renewal_wait(heartbeat.renew()) else {
            return RunnerOutcome::Unavailable;
        };
        let Ok(acquired) = acquire(
            Arc::clone(&self.acquisition),
            request.clone(),
            acquired,
            heartbeat,
            renew_after,
        ) else {
            return RunnerOutcome::Unavailable;
        };
        let Some((evaluation_instant, valid_until)) =
            trusted_window(self.clock.as_ref(), self.validity_seconds)
        else {
            return RunnerOutcome::Unavailable;
        };
        run_bootstrap(
            request,
            BootstrapRun {
                executable: &self.executable,
                repository: acquired.repository.path(),
                action_repository: acquired.action.path(),
                scratch: &self.scratch,
                evaluation_instant: &evaluation_instant,
                valid_until: &valid_until,
                wall_timeout: self.wall_timeout,
            },
            heartbeat,
        )
    }
}

struct AcquiredRun {
    repository: tempfile::TempDir,
    action: tempfile::TempDir,
}

impl AcquiredRun {
    fn new(scratch: &Path) -> std::io::Result<Self> {
        let repository = tempfile::Builder::new()
            .prefix("amiss-repository-")
            .tempdir_in(scratch)?;
        let action = tempfile::Builder::new()
            .prefix("amiss-action-")
            .tempdir_in(scratch)?;
        Ok(Self { repository, action })
    }
}

fn acquire<A>(
    acquisition: Arc<Mutex<Option<A>>>,
    request: RunRequest,
    roots: AcquiredRun,
    heartbeat: &mut dyn RunHeartbeat,
    renew_after: Duration,
) -> Result<AcquiredRun, ()>
where
    A: Acquisition + 'static,
{
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (sender, receiver) = mpsc::sync_channel(1);
    let _worker = std::thread::Builder::new()
        .name("amiss-acquisition".to_owned())
        .spawn(move || {
            let acquired = run_acquisition(&acquisition, &request, &roots, worker_cancelled);
            let _ignored = sender.send((acquired, roots));
        })
        .map_err(|_defect| ())?;
    await_acquisition(&receiver, &cancelled, heartbeat, renew_after)
}

fn run_acquisition<A: Acquisition>(
    slot: &Mutex<Option<A>>,
    request: &RunRequest,
    roots: &AcquiredRun,
    cancelled: Arc<AtomicBool>,
) -> bool {
    let Some(mut acquisition) = slot.lock().ok().and_then(|mut slot| slot.take()) else {
        return false;
    };
    let target = AcquisitionTarget {
        repository: roots.repository.path(),
        action: roots.action.path(),
        cancelled,
    };
    let acquired = acquisition.acquire(request, target).is_ok();
    slot.lock()
        .is_ok_and(|mut slot| slot.replace(acquisition).is_none())
        && acquired
}

fn await_acquisition(
    receiver: &mpsc::Receiver<(bool, AcquiredRun)>,
    cancelled: &AtomicBool,
    heartbeat: &mut dyn RunHeartbeat,
    mut renew_after: Duration,
) -> Result<AcquiredRun, ()> {
    loop {
        match receiver.recv_timeout(renew_after) {
            Ok((true, roots)) => return Ok(roots),
            Ok((false, _roots)) => return Err(()),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(next) = renewal_wait(heartbeat.renew()) else {
                    cancelled.store(true, Ordering::Release);
                    return Err(());
                };
                renew_after = next;
            }
        }
    }
}

fn trusted_window(
    clock: &dyn ControllerClock,
    validity_seconds: i64,
) -> Option<(UtcInstant, UtcInstant)> {
    let now = clock.now_unix_millis().filter(|instant| *instant >= 0)?;
    let evaluation_seconds = now.checked_div(1_000)?;
    let valid_until_seconds = evaluation_seconds.checked_add(validity_seconds)?;
    Some((
        UtcInstant::from_epoch_seconds(evaluation_seconds)?,
        UtcInstant::from_epoch_seconds(valid_until_seconds)?,
    ))
}
