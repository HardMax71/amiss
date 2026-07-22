use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use amiss_bootstrap::result::{BootstrapResult, result_bytes};
use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, SealedControlExpectation, SealedExpectations,
    Supervised, settle, supervise,
};
use amiss_bootstrap::{Refusal, validate};
use amiss_git::{GitLimits, GitResources, ObjectKind, Repository};
use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};
use amiss_wire::json::canonical;
use amiss_wire::report::{MACHINE_JSON_BYTES, WATCHDOG_MILLISECONDS};
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, REQUEST_STREAM_BYTES, RequestMode, RequestStreams,
    SEALED_ENGINE_ARGUMENT, SnapshotRequest,
};

/// The operational wall ceiling from the security contract: the trusted
/// wrapper kills the whole evaluator after 120 seconds, and a killed evaluator
/// yields no accepted result.
const WATCHDOG_CEILING: Duration = Duration::from_millis(WATCHDOG_MILLISECONDS);

#[cfg(windows)]
const PRIVATE_ENGINE_NAME: &str = "engine.exe";

#[cfg(not(windows))]
const PRIVATE_ENGINE_NAME: &str = "engine";

/// The trusted bootstrap, which is also the trusted wrapper the security
/// contract names. It validates the pinned action tree as data, launches the
/// verified engine with a cleared environment and fixed arguments, holds it to
/// the wall ceiling, and publishes only an envelope it can accept. It never
/// runs the action's declared Node launcher, never resolves a binary through
/// `PATH`, and never downloads, installs, or discovers anything.
///
/// `amiss-bootstrap exec --action-repository P --repository P --constraint F
/// --evaluation-request F --snapshot-request F --controls-request F --scratch P
/// --report F --result F`
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    let Some(parsed) = parse_args(&argv) else {
        eprintln!("amiss-bootstrap: invalid-invocation");
        return ExitCode::from(2);
    };
    let completion = execute(&parsed)
        .and_then(|accepted| publish(&parsed.report, accepted))
        .unwrap_or_else(failed_completion);
    if let Some(diagnostic) = completion.diagnostic {
        eprintln!("amiss-bootstrap: {diagnostic}");
    }
    if write_new(&parsed.result, result_bytes(completion.result)).is_err() {
        eprintln!("amiss-bootstrap: result-unavailable");
        return ExitCode::from(2);
    }
    completion.exit
}

#[derive(Clone, Copy)]
struct Failure {
    result: BootstrapResult,
    diagnostic: &'static str,
}

struct Accepted {
    wire: Vec<u8>,
    class: u8,
    result: BootstrapResult,
}

struct Completion {
    result: BootstrapResult,
    exit: ExitCode,
    diagnostic: Option<&'static str>,
}

type Execution<T> = Result<T, Failure>;

#[derive(Clone, Copy)]
enum ReadDefect {
    Unavailable,
    Oversized,
}

const fn unavailable(diagnostic: &'static str) -> Failure {
    Failure {
        result: BootstrapResult::Unavailable,
        diagnostic,
    }
}

const fn tampered(diagnostic: &'static str) -> Failure {
    Failure {
        result: BootstrapResult::TamperedRuntime,
        diagnostic,
    }
}

const fn input_failure(
    defect: ReadDefect,
    unavailable_diagnostic: &'static str,
    invalid_diagnostic: &'static str,
) -> Failure {
    match defect {
        ReadDefect::Unavailable => unavailable(unavailable_diagnostic),
        ReadDefect::Oversized => tampered(invalid_diagnostic),
    }
}

fn failed_completion(failure: Failure) -> Completion {
    Completion {
        result: failure.result,
        exit: ExitCode::from(2),
        diagnostic: Some(failure.diagnostic),
    }
}

fn execute(args: &Args) -> Execution<Accepted> {
    let constraint_bytes = read_input(
        &args.constraint,
        "constraint-unreadable",
        "constraint-invalid",
    )?;
    let constraint = ExecutionConstraintDescriptor::parse(&constraint_bytes)
        .map_err(|_defect| tampered("constraint-invalid"))?;
    let own_path = env::current_exe().map_err(|_defect| unavailable("self-unreadable"))?;
    let own_bytes = std::fs::read(own_path).map_err(|_defect| unavailable("self-unreadable"))?;
    let action = Repository::open(&args.action_repository, constraint.action_object_format)
        .map_err(|_defect| unavailable("action-tree-unavailable"))?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let validated =
        validate(&action, &mut resources, &constraint, &own_bytes).map_err(validation_failure)?;
    let sealed = capture_requests(args, &constraint)?;
    pre_acquired(&args.repository, &sealed.evaluation)
        .map_err(|()| unavailable("repository-not-pre-acquired"))?;
    run_engine(args, &validated, sealed)
}

const fn validation_failure(refusal: Refusal) -> Failure {
    match refusal {
        Refusal::Unavailable(diagnostic) => unavailable(diagnostic),
        Refusal::Tampered(diagnostic) => tampered(diagnostic),
    }
}

struct Args {
    action_repository: PathBuf,
    repository: PathBuf,
    constraint: PathBuf,
    evaluation_request: PathBuf,
    snapshot_request: PathBuf,
    controls_request: PathBuf,
    scratch: PathBuf,
    report: PathBuf,
    result: PathBuf,
}

fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut action_repository: Option<PathBuf> = None;
    let mut repository: Option<PathBuf> = None;
    let mut constraint: Option<PathBuf> = None;
    let mut evaluation_request: Option<PathBuf> = None;
    let mut snapshot_request: Option<PathBuf> = None;
    let mut controls_request: Option<PathBuf> = None;
    let mut scratch: Option<PathBuf> = None;
    let mut report: Option<PathBuf> = None;
    let mut result: Option<PathBuf> = None;
    let mut items = argv.iter();
    if items.next()? != "exec" {
        return None;
    }
    while let Some(flag) = items.next() {
        let value = items.next()?;
        let slot = match flag.to_str()? {
            "--action-repository" => &mut action_repository,
            "--repository" => &mut repository,
            "--constraint" => &mut constraint,
            "--evaluation-request" => &mut evaluation_request,
            "--snapshot-request" => &mut snapshot_request,
            "--controls-request" => &mut controls_request,
            "--scratch" => &mut scratch,
            "--report" => &mut report,
            "--result" => &mut result,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(PathBuf::from(value));
    }
    let scratch = scratch?;
    if !scratch.is_absolute()
        || !std::fs::symlink_metadata(&scratch).is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return None;
    }
    let report = report?;
    let result = result?;
    if report == result || !new_absolute_path(&report) || !new_absolute_path(&result) {
        return None;
    }
    Some(Args {
        action_repository: action_repository?,
        repository: repository?,
        constraint: constraint?,
        evaluation_request: evaluation_request?,
        snapshot_request: snapshot_request?,
        controls_request: controls_request?,
        scratch,
        report,
        result,
    })
}

fn new_absolute_path(path: &std::path::Path) -> bool {
    path.is_absolute()
        && std::fs::symlink_metadata(path)
            .is_err_and(|defect| defect.kind() == std::io::ErrorKind::NotFound)
}

#[derive(Clone)]
struct SealedRun {
    streams: RequestStreams,
    evaluation: EvaluationRequest,
    expected: SealedExpectations,
}

fn capture_requests(
    args: &Args,
    constraint: &ExecutionConstraintDescriptor,
) -> Execution<SealedRun> {
    let streams = request_streams(args)?;
    let evaluation = EvaluationRequest::parse(&streams.evaluation)
        .map_err(|_defect| tampered("evaluation-request-invalid"))?;
    let snapshot = SnapshotRequest::parse(&streams.snapshot)
        .map_err(|_defect| tampered("snapshot-request-invalid"))?;
    let controls = ControlsRequest::parse(&streams.controls)
        .map_err(|_defect| tampered("controls-request-invalid"))?;
    let canonical_requests = evaluation.canonical_bytes().ok().as_deref()
        == Some(streams.evaluation.as_slice())
        && snapshot.canonical_bytes().ok().as_deref() == Some(streams.snapshot.as_slice())
        && controls.canonical_bytes().ok().as_deref() == Some(streams.controls.as_slice());
    if !canonical_requests {
        return Err(tampered("request-noncanonical"));
    }
    let candidate = match (evaluation.mode, evaluation.candidate_commit.as_ref()) {
        (RequestMode::CommitPair, Some(candidate))
            if snapshot.materialization == RequestMode::CommitPair =>
        {
            candidate.clone()
        }
        (RequestMode::CommitPair | RequestMode::Index, None | Some(_)) => {
            return Err(tampered("request-mode-mismatch"));
        }
    };
    let repository = sealed_identity(&evaluation).map_err(tampered)?;
    let supplied_constraint = controls
        .execution_constraint
        .as_ref()
        .ok_or_else(|| tampered("execution-constraint-absent"))?;
    let embedded_constraint =
        ExecutionConstraintDescriptor::parse(&canonical(&supplied_constraint.value))
            .map_err(|_defect| tampered("execution-constraint-invalid"))?;
    if embedded_constraint.digest != supplied_constraint.expected_digest
        || embedded_constraint != *constraint
    {
        return Err(tampered("execution-constraint-mismatch"));
    }
    let supplied_time = controls
        .trusted_time
        .as_ref()
        .ok_or_else(|| tampered("trusted-time-absent"))?;
    let statement = TrustedTimeStatement::parse(&canonical(&supplied_time.value))
        .map_err(|_defect| tampered("trusted-time-invalid"))?;
    if statement.digest != supplied_time.expected_digest
        || statement.provider != supplied_time.provider
        || statement.provider_run_id != supplied_time.provider_run_id
        || statement.provider_run_attempt != supplied_time.provider_run_attempt
    {
        return Err(tampered("trusted-time-mismatch"));
    }
    let expected = SealedExpectations {
        profile: match evaluation.profile {
            amiss_wire::controls::Profile::Observe => "observe",
            amiss_wire::controls::Profile::Enforce => "enforce",
        }
        .to_owned(),
        candidate_ref: evaluation
            .candidate_ref
            .as_ref()
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
        target_ref: evaluation
            .target_ref
            .as_ref()
            .map_or_else(String::new, |reference| reference.as_str().to_owned()),
        repository,
        provider: supplied_time.provider.clone(),
        provider_run_id: supplied_time.provider_run_id.clone(),
        provider_run_attempt: supplied_time.provider_run_attempt,
        candidate_identity_digest: statement.candidate_identity_digest.to_string(),
        organization_floor: control_expectation(controls.organization_floor.as_ref()),
        debt_snapshot: control_expectation(controls.debt_snapshot.as_ref()),
        waiver_bundle: control_expectation(controls.waiver_bundle.as_ref()),
        execution_constraint: SealedControlExpectation {
            digest: constraint.digest.to_string(),
            trust_source: supplied_constraint.trust_source.as_str().to_owned(),
        },
        trusted_time_digest: statement.digest.to_string(),
    };
    let mut evaluation = evaluation;
    evaluation.candidate_commit = Some(candidate);
    Ok(SealedRun {
        streams,
        evaluation,
        expected,
    })
}

fn request_streams(args: &Args) -> Execution<RequestStreams> {
    let streams = RequestStreams {
        evaluation: read_input(
            &args.evaluation_request,
            "evaluation-request-unreadable",
            "evaluation-request-invalid",
        )?,
        snapshot: read_input(
            &args.snapshot_request,
            "snapshot-request-unreadable",
            "snapshot-request-invalid",
        )?,
        controls: read_input(
            &args.controls_request,
            "controls-request-unreadable",
            "controls-request-invalid",
        )?,
    };
    Ok(streams)
}

fn sealed_identity(
    evaluation: &EvaluationRequest,
) -> Result<amiss_wire::model::RepositoryIdentity, &'static str> {
    let Some(repository) = evaluation.repository.clone() else {
        return Err("evaluation-identity-absent");
    };
    if evaluation.forge.is_none()
        || evaluation.candidate_ref.is_none()
        || evaluation.target_ref.is_none()
        || evaluation.default_branch_ref.is_none()
    {
        return Err("evaluation-identity-absent");
    }
    Ok(repository)
}

fn control_expectation(
    supplied: Option<&amiss_wire::requests::SuppliedControl>,
) -> Option<SealedControlExpectation> {
    supplied.map(|control| SealedControlExpectation {
        digest: control.expected_digest.to_string(),
        trust_source: control.trust_source.as_str().to_owned(),
    })
}

fn read_bounded(path: &std::path::Path) -> Result<Vec<u8>, ReadDefect> {
    let file = std::fs::File::open(path).map_err(|_defect| ReadDefect::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(REQUEST_STREAM_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_defect| ReadDefect::Unavailable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return Err(ReadDefect::Oversized);
    }
    Ok(bytes)
}

fn read_input(
    path: &std::path::Path,
    unavailable_diagnostic: &'static str,
    invalid_diagnostic: &'static str,
) -> Execution<Vec<u8>> {
    read_bounded(path)
        .map_err(|defect| input_failure(defect, unavailable_diagnostic, invalid_diagnostic))
}

fn pre_acquired(path: &std::path::Path, evaluation: &EvaluationRequest) -> Result<(), ()> {
    let repository = Repository::open(path, evaluation.object_format).map_err(|_defect| ())?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    repository
        .read_expected(&mut resources, &evaluation.base_commit, ObjectKind::Commit)
        .map_err(|_defect| ())?;
    let candidate = evaluation.candidate_commit.as_ref().ok_or(())?;
    repository
        .read_expected(&mut resources, candidate, ObjectKind::Commit)
        .map_err(|_defect| ())?;
    Ok(())
}

/// Writes the verified engine bytes into a private directory and launches them
/// with an empty environment. The bytes come from the validated tree, never
/// from a worktree file, a `PATH` lookup, or the action's launcher.
fn run_engine(
    args: &Args,
    validated: &amiss_bootstrap::Validated,
    sealed: SealedRun,
) -> Execution<Accepted> {
    let expectations = Expectations {
        engine_digest: validated.engine_digest.to_string(),
        base_commit: sealed.evaluation.base_commit.as_str().to_owned(),
        candidate_commit: sealed
            .evaluation
            .candidate_commit
            .as_ref()
            .map(|candidate| candidate.as_str().to_owned()),
        sealed: Some(sealed.expected.clone()),
    };

    let private = tempfile::TempDir::new_in(&args.scratch)
        .map_err(|_defect| unavailable("private-storage-unavailable"))?;
    let engine = private.path().join(PRIVATE_ENGINE_NAME);
    std::fs::write(&engine, &validated.binary)
        .map_err(|_defect| unavailable("private-storage-unavailable"))?;
    executable_bit(&engine).map_err(|_defect| unavailable("private-storage-unavailable"))?;

    let mut child = std::process::Command::new(&engine)
        .arg(SEALED_ENGINE_ARGUMENT)
        .current_dir(&args.repository)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_defect| unavailable("engine-launch-failed"))?;
    let (outcome, wire) = collect(&mut child, sealed.streams)
        .map_err(|_defect| unavailable("engine-collection-failed"))?;
    let class = settle(&outcome, &wire, &expectations)
        .map_err(|defect| settlement_failure(defect, wire.is_empty()))?;
    let (class, result) = match class {
        0 => (0, BootstrapResult::Pass),
        1 => (1, BootstrapResult::Block),
        _ => return Err(tampered("report-exit-class")),
    };
    Ok(Accepted {
        wire,
        class,
        result,
    })
}

/// Drains the engine's stdout while the watchdog runs. A supervisor that only
/// polls would deadlock the moment the engine's report outgrew the pipe
/// buffer: the engine would block writing, never exit, and be killed for a
/// slowness that was the supervisor's own.
fn collect(
    child: &mut std::process::Child,
    requests: RequestStreams,
) -> std::io::Result<(Supervised, Vec<u8>)> {
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no engine stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no engine stdout"))?;
    let writer = std::thread::spawn(move || {
        requests.write_to(&mut stdin)?;
        stdin.flush()
    });
    let reader = std::thread::spawn(move || {
        let mut wire = Vec::new();
        let mut bounded = stdout.take(MACHINE_JSON_BYTES.saturating_add(1));
        bounded.read_to_end(&mut wire).map(|_count| wire)
    });
    let outcome = match supervise(child, WATCHDOG_CEILING) {
        Ok(outcome) => outcome,
        Err(defect) => {
            let _signalled = child.kill();
            let _reaped = child.wait();
            let _writer = writer.join();
            let _reader = reader.join();
            return Err(defect);
        }
    };
    let write_result = writer
        .join()
        .map_err(|_panic| std::io::Error::other("engine request writer failed"));
    if !matches!(outcome, Supervised::Killed) {
        write_result??;
    }
    let wire = reader
        .join()
        .map_err(|_panic| std::io::Error::other("engine reader failed"))??;
    Ok((outcome, wire))
}

/// Publishes the accepted envelope before exposing its result record.
fn publish(path: &std::path::Path, accepted: Accepted) -> Execution<Completion> {
    let Accepted {
        wire,
        class,
        result,
    } = accepted;
    write_new(path, &wire).map_err(|_defect| unavailable("report-publish-failed"))?;
    Ok(Completion {
        result,
        exit: ExitCode::from(class),
        diagnostic: None,
    })
}

fn write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let written = file.write_all(bytes).and_then(|()| file.flush());
    if let Err(defect) = written {
        drop(file);
        let _removed = std::fs::remove_file(path);
        return Err(defect);
    }
    Ok(())
}

const fn settlement_failure(defect: Defect, empty: bool) -> Failure {
    match defect {
        Defect::Killed => Failure {
            result: BootstrapResult::Timeout,
            diagnostic: "evaluator-watchdog-kill",
        },
        Defect::Signalled => unavailable("evaluator-signalled"),
        Defect::Oversize => Failure {
            result: BootstrapResult::OversizedOutput,
            diagnostic: "report-over-wire-ceiling",
        },
        Defect::ExitMismatch => tampered("evaluator-exit-mismatch"),
        Defect::Acceptance(_defect) if empty => Failure {
            result: BootstrapResult::MissingOutput,
            diagnostic: "report-missing",
        },
        Defect::Acceptance(defect) => tampered(refused(Defect::Acceptance(defect))),
    }
}

const fn refused(defect: Defect) -> &'static str {
    match defect {
        Defect::Killed => "evaluator-watchdog-kill",
        Defect::Signalled => "evaluator-signalled",
        Defect::Oversize => "report-over-wire-ceiling",
        Defect::ExitMismatch => "evaluator-exit-mismatch",
        Defect::Acceptance(AcceptanceDefect::Shape) => "report-shape",
        Defect::Acceptance(AcceptanceDefect::Noncanonical) => "report-noncanonical",
        Defect::Acceptance(AcceptanceDefect::PayloadDigest) => "report-payload-digest",
        Defect::Acceptance(AcceptanceDefect::Engine) => "report-engine-mismatch",
        Defect::Acceptance(AcceptanceDefect::BaseIdentity) => "report-base-mismatch",
        Defect::Acceptance(AcceptanceDefect::CandidateIdentity) => "report-candidate-mismatch",
        Defect::Acceptance(AcceptanceDefect::SealedIdentity) => "report-request-mismatch",
        Defect::Acceptance(AcceptanceDefect::SealedControls) => "report-controls-mismatch",
        Defect::Acceptance(AcceptanceDefect::Completeness) => "report-completeness",
        Defect::Acceptance(AcceptanceDefect::FindingCount) => "report-finding-count",
    }
}

#[cfg(unix)]
fn executable_bit(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn executable_bit(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
