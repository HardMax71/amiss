use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use amiss_bootstrap::result::{BootstrapResult, result_bytes, result_exit_code};
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::report::MACHINE_JSON_BYTES;

const MALFORMED_RESULT: &[u8] = b"not-an-amiss-bootstrap-result\n";
const STARTED_MARKER: &str = "runner-started";
const GRANDCHILD_READY_MARKER: &str = "runner-ready";
const GRANDCHILD_MARKER: &str = "runner-escaped";
const GRANDCHILD_LOCK: &str = "runner-lock";
const REPLACED_MARKER: &str = "runner-replaced";
const RENEWAL_GATE: &str = "runner-renewal-gate";
const GRANDCHILD_DELAY: Duration = Duration::from_millis(500);
const GRANDCHILD_READY_TIMEOUT: Duration = Duration::from_secs(2);
const GRANDCHILD_READY_POLL: Duration = Duration::from_millis(5);
const COMPLETION_DELAY: Duration = Duration::from_millis(100);

fn main() -> ExitCode {
    let mut invocation = env::args_os().skip(1);
    if invocation.next().as_deref() == Some(OsStr::new("--grandchild")) {
        return grandchild(invocation);
    }
    let Some(args) = runner_args(env::args_os().skip(1)) else {
        return ExitCode::from(2);
    };
    if !valid_layout(&args) {
        return malformed(&args.result);
    }
    if write_marker(&args.repository.join(STARTED_MARKER)).is_err() {
        return malformed(&args.result);
    }
    let Some(mode) = read_mode(&args.constraint) else {
        return malformed(&args.result);
    };
    run(mode, &args)
}

struct RunnerArgs {
    action_repository: PathBuf,
    repository: PathBuf,
    constraint: PathBuf,
    evaluation: PathBuf,
    snapshot: PathBuf,
    controls: PathBuf,
    scratch: PathBuf,
    report: PathBuf,
    result: PathBuf,
}

#[derive(Clone, Copy)]
enum Mode {
    Pass,
    Block,
    MissingResult,
    MalformedResult,
    OversizedOutput,
    Timeout,
    ClearedEnvironment,
    DelayedPass,
    RenewedPass,
    ExitWithChild,
    ReplaceOutputs,
}

fn runner_args(mut argv: impl Iterator<Item = OsString>) -> Option<RunnerArgs> {
    literal(&mut argv, "exec")?;
    let action_repository = argument(&mut argv, "--action-repository")?;
    let repository = argument(&mut argv, "--repository")?;
    let constraint = argument(&mut argv, "--constraint")?;
    let evaluation = argument(&mut argv, "--evaluation-request")?;
    let snapshot = argument(&mut argv, "--snapshot-request")?;
    let controls = argument(&mut argv, "--controls-request")?;
    let scratch = argument(&mut argv, "--scratch")?;
    let report = argument(&mut argv, "--report")?;
    let result = argument(&mut argv, "--result")?;
    if argv.next().is_some() {
        return None;
    }
    Some(RunnerArgs {
        action_repository,
        repository,
        constraint,
        evaluation,
        snapshot,
        controls,
        scratch,
        report,
        result,
    })
}

fn literal(argv: &mut impl Iterator<Item = OsString>, expected: &str) -> Option<()> {
    (argv.next()?.as_os_str() == OsStr::new(expected)).then_some(())
}

fn argument(argv: &mut impl Iterator<Item = OsString>, expected: &str) -> Option<PathBuf> {
    literal(argv, expected)?;
    Some(PathBuf::from(argv.next()?))
}

fn valid_layout(args: &RunnerArgs) -> bool {
    let Some(directory) = args.constraint.parent() else {
        return false;
    };
    args.action_repository.is_absolute()
        && args.repository.is_absolute()
        && directory.is_absolute()
        && regular_directory(&args.action_repository)
        && regular_directory(&args.repository)
        && args.scratch == directory
        && regular_directory(&args.scratch)
        && input(&args.constraint, directory, "constraint.json")
        && input(&args.evaluation, directory, "evaluation.json")
        && input(&args.snapshot, directory, "snapshot.json")
        && input(&args.controls, directory, "controls.json")
        && output(&args.report, directory, "report")
        && output(&args.result, directory, "result")
        && env::current_dir().is_ok_and(|current| same_directory(&current, directory))
}

fn same_directory(left: &Path, right: &Path) -> bool {
    same_file::is_same_file(left, right).unwrap_or(false)
}

fn regular_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

fn input(path: &Path, directory: &Path, name: &str) -> bool {
    path.is_absolute()
        && path.parent() == Some(directory)
        && path.file_name() == Some(OsStr::new(name))
        && std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn output(path: &Path, directory: &Path, name: &str) -> bool {
    path.is_absolute()
        && path.parent() == Some(directory)
        && path.file_name() == Some(OsStr::new(name))
        && std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() == 0)
}

fn read_mode(path: &Path) -> Option<Mode> {
    let bytes = std::fs::read(path).ok()?;
    let constraint = ExecutionConstraintDescriptor::parse(&bytes).ok()?;
    match constraint.required_status_name.as_str() {
        "runner-pass" => Some(Mode::Pass),
        "runner-block" => Some(Mode::Block),
        "runner-missing" => Some(Mode::MissingResult),
        "runner-malformed" => Some(Mode::MalformedResult),
        "runner-oversized" => Some(Mode::OversizedOutput),
        "runner-hang" => Some(Mode::Timeout),
        "runner-environment" => Some(Mode::ClearedEnvironment),
        "runner-delayed-pass" => Some(Mode::DelayedPass),
        "runner-renewed-pass" => Some(Mode::RenewedPass),
        "runner-exit-child" => Some(Mode::ExitWithChild),
        "runner-replace-outputs" => Some(Mode::ReplaceOutputs),
        _ => None,
    }
}

fn run(mode: Mode, args: &RunnerArgs) -> ExitCode {
    match mode {
        Mode::Pass => complete(
            &args.report,
            &args.result,
            BootstrapResult::Pass,
            b"{\"runner\":\"pass\"}\n",
        ),
        Mode::Block => complete(
            &args.report,
            &args.result,
            BootstrapResult::Block,
            b"{\"runner\":\"block\"}\n",
        ),
        Mode::MissingResult => ExitCode::SUCCESS,
        Mode::OversizedOutput => oversized(&args.report, &args.result),
        Mode::Timeout => timeout(args),
        Mode::ClearedEnvironment if cleared_environment() => complete(
            &args.report,
            &args.result,
            BootstrapResult::Pass,
            b"{\"runner\":\"pass\"}\n",
        ),
        Mode::DelayedPass => {
            std::thread::sleep(COMPLETION_DELAY);
            complete(
                &args.report,
                &args.result,
                BootstrapResult::Pass,
                b"{\"runner\":\"pass\"}\n",
            )
        }
        Mode::RenewedPass if renewal_gate(args) => complete(
            &args.report,
            &args.result,
            BootstrapResult::Pass,
            b"{\"runner\":\"pass\"}\n",
        ),
        Mode::ExitWithChild => exit_with_child(args),
        Mode::ReplaceOutputs => replace_outputs(args),
        Mode::MalformedResult | Mode::ClearedEnvironment | Mode::RenewedPass => {
            malformed(&args.result)
        }
    }
}

fn cleared_environment() -> bool {
    env::var_os("PATH").is_none()
}

fn renewal_gate(args: &RunnerArgs) -> bool {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(args.repository.join(RENEWAL_GATE))
        .and_then(|gate| gate.lock())
        .is_ok()
}

fn complete(report: &Path, result: &Path, outcome: BootstrapResult, bytes: &[u8]) -> ExitCode {
    if write_output(report, bytes).is_err() || write_output(result, result_bytes(outcome)).is_err()
    {
        return ExitCode::from(2);
    }
    u8::try_from(result_exit_code(outcome)).map_or(ExitCode::from(2), ExitCode::from)
}

fn malformed(result: &Path) -> ExitCode {
    if write_output(result, MALFORMED_RESULT).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn oversized(report: &Path, result: &Path) -> ExitCode {
    let Ok(output) = OpenOptions::new().write(true).open(report) else {
        return ExitCode::from(2);
    };
    if output
        .set_len(MACHINE_JSON_BYTES.saturating_add(2))
        .is_err()
    {
        return ExitCode::from(2);
    }
    drop(output);
    if write_output(result, result_bytes(BootstrapResult::Pass)).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn timeout(args: &RunnerArgs) -> ExitCode {
    if !spawn_grandchild(args) {
        return malformed(&args.result);
    }
    loop {
        std::thread::sleep(Duration::from_mins(1));
    }
}

fn exit_with_child(args: &RunnerArgs) -> ExitCode {
    if !spawn_grandchild(args) {
        return malformed(&args.result);
    }
    complete(
        &args.report,
        &args.result,
        BootstrapResult::Pass,
        b"{\"runner\":\"pass\"}\n",
    )
}

fn replace_outputs(args: &RunnerArgs) -> ExitCode {
    let replaced = std::fs::remove_file(&args.report)
        .and_then(|()| std::fs::remove_file(&args.result))
        .and_then(|()| write_new(&args.report, b"{\"runner\":\"pass\"}\n"))
        .and_then(|()| write_new(&args.result, result_bytes(BootstrapResult::Pass)))
        .and_then(|()| write_new(&args.repository.join(REPLACED_MARKER), b"replaced\n"));
    if replaced.is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn spawn_grandchild(args: &RunnerArgs) -> bool {
    let ready = args.repository.join(GRANDCHILD_READY_MARKER);
    let marker = args.repository.join(GRANDCHILD_MARKER);
    let lock = args.repository.join(GRANDCHILD_LOCK);
    let Ok(executable) = env::current_exe() else {
        return false;
    };
    let mut command = Command::new(executable);
    command
        .arg("--grandchild")
        .arg(&ready)
        .arg(marker)
        .arg(lock)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if command.spawn().is_err() {
        return false;
    }
    wait_for_ready(&ready)
}

fn wait_for_ready(path: &Path) -> bool {
    let deadline = Instant::now() + GRANDCHILD_READY_TIMEOUT;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(GRANDCHILD_READY_POLL);
    }
    path.exists()
}

fn grandchild(mut argv: impl Iterator<Item = OsString>) -> ExitCode {
    let Some(ready) = argv.next().map(PathBuf::from) else {
        return ExitCode::from(2);
    };
    let Some(marker) = argv.next().map(PathBuf::from) else {
        return ExitCode::from(2);
    };
    let Some(lock_path) = argv.next().map(PathBuf::from) else {
        return ExitCode::from(2);
    };
    if argv.next().is_some()
        || !ready.is_absolute()
        || !marker.is_absolute()
        || !lock_path.is_absolute()
    {
        return ExitCode::from(2);
    }
    let Ok(lock) = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_path)
    else {
        return ExitCode::from(2);
    };
    if lock.lock().is_err() {
        return ExitCode::from(2);
    }
    if write_new(&ready, b"ready\n").is_err() {
        return ExitCode::from(2);
    }
    std::thread::sleep(GRANDCHILD_DELAY);
    if write_new(&marker, b"escaped\n").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn write_output(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn write_marker(path: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(b"started\n")?;
    file.flush()
}
