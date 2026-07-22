use std::fs::{File, OpenOptions};
use std::io::{Read, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_bootstrap::result::RESULT_BYTES;
use amiss_wire::digest::hb;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::{MACHINE_JSON_BYTES, WATCHDOG_MILLISECONDS};
use processkit::{
    CancellationToken, Command, Error as ProcessError, ProcessGroup, Stdin, StdioMode,
};

use crate::{
    AcquiredRoots, BootstrapJob, BootstrapJobInput, BootstrapTermination, HeartbeatOutcome,
    RunHeartbeat, RunRequest, RunnerOutcome, bootstrap_job, classify_bootstrap_result,
    verify_acquired,
};

const MAX_HEARTBEAT_WAIT: Duration = Duration::from_secs(5);
const PROCESS_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const PROCESS_DRAIN_POLL: Duration = Duration::from_millis(10);
const BOOTSTRAP_EXECUTABLE_BYTES: u64 = 33_554_432;

/// The trusted paths, sealed input, and time bounds for one bootstrap process.
#[derive(Clone, Copy, Debug)]
pub struct BootstrapRun<'a> {
    pub executable: &'a Path,
    pub repository: &'a Path,
    pub action_repository: &'a Path,
    pub scratch: &'a Path,
    pub evaluation_instant: &'a UtcInstant,
    pub valid_until: &'a UtcInstant,
    pub wall_timeout: Duration,
}

struct PreparedRun {
    // Output handles close before directory cleanup.
    report: OutputFile,
    result: OutputFile,
    directory: tempfile::TempDir,
    executable: PathBuf,
    constraint: PathBuf,
    evaluation: PathBuf,
    snapshot: PathBuf,
    controls: PathBuf,
}

struct OutputFile {
    path: PathBuf,
    file: File,
}

/// Verifies the acquired trees, runs the trusted bootstrap in a contained
/// process tree, and classifies its bounded result channel.
#[must_use]
pub fn run_bootstrap(
    request: &RunRequest,
    run: BootstrapRun<'_>,
    heartbeat: &mut dyn RunHeartbeat,
) -> RunnerOutcome {
    if !valid_run(&run) {
        return RunnerOutcome::Unavailable;
    }
    if verify_acquired(
        request,
        AcquiredRoots {
            repository: run.repository,
            action: run.action_repository,
        },
    )
    .is_err()
    {
        return RunnerOutcome::TamperedRuntime;
    }
    let Ok(executable) = read_bounded(run.executable, BOOTSTRAP_EXECUTABLE_BYTES) else {
        return RunnerOutcome::Unavailable;
    };
    if hb(BOOTSTRAP_DOMAIN, &executable) != request.plan.execution.bootstrap_digest {
        return RunnerOutcome::TamperedRuntime;
    }
    let Ok(job) = bootstrap_job(BootstrapJobInput {
        run: request,
        evaluation_instant: run.evaluation_instant.clone(),
        valid_until: run.valid_until.clone(),
    }) else {
        return RunnerOutcome::TamperedRuntime;
    };
    let Ok(mut prepared) = prepare(&run, &job, &executable) else {
        return RunnerOutcome::Unavailable;
    };
    let Some(renew_after) = renewal_wait(heartbeat.renew()) else {
        return RunnerOutcome::Unavailable;
    };
    let Ok(termination) = supervise(&run, &prepared, renew_after, heartbeat) else {
        return RunnerOutcome::Unavailable;
    };
    let (result, report) = match termination {
        BootstrapTermination::Exited(_code) => match read_result(&mut prepared.result.file) {
            Ok(None) => (None, Vec::new()),
            Ok(Some(result)) => match read_report(&mut prepared.report.file) {
                Ok(report) => (Some(result), report),
                Err(_defect) => return RunnerOutcome::Unavailable,
            },
            Err(_defect) => return RunnerOutcome::Unavailable,
        },
        BootstrapTermination::TimedOut
        | BootstrapTermination::HeartbeatStopped
        | BootstrapTermination::Signalled
        | BootstrapTermination::SpawnUnavailable => (None, Vec::new()),
    };
    classify_bootstrap_result(request, termination, result, report)
}

fn valid_run(run: &BootstrapRun<'_>) -> bool {
    run.wall_timeout > Duration::ZERO
        && run.wall_timeout <= Duration::from_millis(WATCHDOG_MILLISECONDS)
        && regular_file(run.executable)
        && directory(run.repository)
        && directory(run.action_repository)
        && directory(run.scratch)
}

fn regular_file(path: &Path) -> bool {
    path.is_absolute()
        && std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn directory(path: &Path) -> bool {
    path.is_absolute()
        && std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

fn prepare(
    run: &BootstrapRun<'_>,
    job: &BootstrapJob,
    executable_bytes: &[u8],
) -> std::io::Result<PreparedRun> {
    let directory = tempfile::Builder::new()
        .prefix("amiss-controller-")
        .tempdir_in(run.scratch)?;
    let executable_name = run
        .executable
        .file_name()
        .ok_or_else(|| std::io::Error::other("bootstrap executable has no file name"))?;
    let executable = directory.path().join(executable_name);
    let constraint = directory.path().join("constraint.json");
    let evaluation = directory.path().join("evaluation.json");
    let snapshot = directory.path().join("snapshot.json");
    let controls = directory.path().join("controls.json");
    let report = directory.path().join("report");
    let result = directory.path().join("result");
    write_new(&executable, executable_bytes)?;
    std::fs::set_permissions(
        &executable,
        std::fs::metadata(run.executable)?.permissions(),
    )?;
    write_new(&constraint, &job.constraint)?;
    write_new(&evaluation, &job.streams.evaluation)?;
    write_new(&snapshot, &job.streams.snapshot)?;
    write_new(&controls, &job.streams.controls)?;
    let report = create_output(report)?;
    let result = create_output(result)?;
    Ok(PreparedRun {
        report,
        result,
        directory,
        executable,
        constraint,
        evaluation,
        snapshot,
        controls,
    })
}

fn create_output(path: PathBuf) -> std::io::Result<OutputFile> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(OutputFile { path, file })
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn supervise(
    run: &BootstrapRun<'_>,
    prepared: &PreparedRun,
    renew_after: Duration,
    heartbeat: &mut dyn RunHeartbeat,
) -> Result<BootstrapTermination, ()> {
    let cancelled = CancellationToken::new();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let worker_cancelled = cancelled.clone();
        let worker = std::thread::Builder::new()
            .name("amiss-bootstrap-supervisor".to_owned())
            .spawn_scoped(scope, move || {
                let result = process(run, prepared, worker_cancelled);
                let _ignored = sender.send(result);
            })
            .map_err(|_defect| ())?;
        let result = receive(&receiver, &cancelled, renew_after, heartbeat);
        let joined = worker.join();
        match joined {
            Ok(()) => result?.map_err(|_defect| ()),
            Err(_panic) => Err(()),
        }
    })
}

fn receive(
    receiver: &mpsc::Receiver<std::io::Result<BootstrapTermination>>,
    cancelled: &CancellationToken,
    mut renew_after: Duration,
    heartbeat: &mut dyn RunHeartbeat,
) -> Result<std::io::Result<BootstrapTermination>, ()> {
    loop {
        match receiver.recv_timeout(renew_after) {
            Ok(captured) => return Ok(captured),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(next_renewal) = renewal_wait(heartbeat.renew()) else {
                    cancelled.cancel();
                    let finished = receiver.recv().map_err(|_defect| ())?;
                    return Ok(finished.map(|_termination| BootstrapTermination::HeartbeatStopped));
                };
                renew_after = next_renewal;
            }
        }
    }
}

fn renewal_wait(outcome: HeartbeatOutcome) -> Option<Duration> {
    match outcome {
        HeartbeatOutcome::Renewed { renew_within } => {
            let wait = renew_within / 2;
            (!wait.is_zero()).then_some(wait.min(MAX_HEARTBEAT_WAIT))
        }
        HeartbeatOutcome::Stop => None,
    }
}

fn process(
    run: &BootstrapRun<'_>,
    prepared: &PreparedRun,
    cancelled: CancellationToken,
) -> std::io::Result<BootstrapTermination> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(contained_process(run, prepared, cancelled))
}

async fn contained_process(
    run: &BootstrapRun<'_>,
    prepared: &PreparedRun,
    cancelled: CancellationToken,
) -> std::io::Result<BootstrapTermination> {
    let group = ProcessGroup::new().map_err(std::io::Error::other)?;
    let command = command(run, prepared).cancel_on(cancelled);
    let termination = match group.start(&command).await {
        Ok(process) => process
            .wait()
            .await
            .map_or_else(|error| process_failure(&error), process_outcome),
        Err(error) => process_failure(&error),
    };
    if stop_tree(&group).await {
        Ok(termination)
    } else {
        Ok(BootstrapTermination::SpawnUnavailable)
    }
}

async fn stop_tree(group: &ProcessGroup) -> bool {
    group.kill_all().is_ok()
        && tokio::time::timeout(PROCESS_DRAIN_TIMEOUT, async {
            loop {
                if group.members().is_ok_and(|members| members.is_empty()) {
                    return true;
                }
                tokio::time::sleep(PROCESS_DRAIN_POLL).await;
            }
        })
        .await
        .unwrap_or(false)
}

fn command(run: &BootstrapRun<'_>, prepared: &PreparedRun) -> Command {
    let arguments: [(&str, &Path); 9] = [
        ("--action-repository", run.action_repository),
        ("--repository", run.repository),
        ("--constraint", &prepared.constraint),
        ("--evaluation-request", &prepared.evaluation),
        ("--snapshot-request", &prepared.snapshot),
        ("--controls-request", &prepared.controls),
        ("--scratch", prepared.directory.path()),
        ("--report", &prepared.report.path),
        ("--result", &prepared.result.path),
    ];
    arguments
        .into_iter()
        .fold(
            Command::new(&prepared.executable).arg("exec"),
            |command, (flag, value)| command.arg(flag).arg(value),
        )
        .current_dir(prepared.directory.path())
        .env_clear()
        .stdin(Stdin::empty())
        .stdout(StdioMode::Null)
        .stderr(StdioMode::Null)
        .timeout(run.wall_timeout)
}

fn process_failure(error: &ProcessError) -> BootstrapTermination {
    if let ProcessError::Cancelled { .. } = error {
        BootstrapTermination::HeartbeatStopped
    } else {
        BootstrapTermination::SpawnUnavailable
    }
}

fn process_outcome(outcome: processkit::Outcome) -> BootstrapTermination {
    match outcome.code() {
        Some(code) => BootstrapTermination::Exited(code),
        None if outcome.timed_out() => BootstrapTermination::TimedOut,
        None => BootstrapTermination::Signalled,
    }
}

fn read_result(file: &mut File) -> std::io::Result<Option<Vec<u8>>> {
    let bytes = read_output(file, RESULT_BYTES.saturating_add(1))?;
    Ok((!bytes.is_empty()).then_some(bytes))
}

fn read_report(file: &mut File) -> std::io::Result<Vec<u8>> {
    read_output(file, MACHINE_JSON_BYTES.saturating_add(1))
}

fn read_output(file: &mut File, limit: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    read_at_most(file, limit)
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let bytes = read_at_most(&mut file, limit.saturating_add(1))?;
    if u64::try_from(bytes.len()).map_or(true, |size| size > limit) {
        return Err(std::io::Error::other("bootstrap executable too large"));
    }
    Ok(bytes)
}

fn read_at_most(file: &mut impl Read, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8_192];
    loop {
        let current = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let available = limit.saturating_sub(current);
        if available == 0 {
            return Ok(bytes);
        }
        let count = usize::try_from(available.min(u64::try_from(buffer.len()).unwrap_or(u64::MAX)))
            .map_err(std::io::Error::other)?;
        let chunk = buffer
            .get_mut(..count)
            .ok_or_else(|| std::io::Error::other("bootstrap read bound"))?;
        let read = file.read(chunk)?;
        if read == 0 {
            return Ok(bytes);
        }
        let chunk = chunk
            .get(..read)
            .ok_or_else(|| std::io::Error::other("bootstrap read count"))?;
        bytes
            .try_reserve_exact(read)
            .map_err(std::io::Error::other)?;
        bytes.extend_from_slice(chunk);
    }
}
