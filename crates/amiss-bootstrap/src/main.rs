use std::env;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

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
/// --evaluation-request F --snapshot-request F --controls-request F`
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(2);
    let Some(parsed) = parse_args(&argv) else {
        eprintln!("amiss-bootstrap: invalid-invocation");
        return failure;
    };
    let Ok(constraint_bytes) = read_bounded(&parsed.constraint) else {
        eprintln!("amiss-bootstrap: constraint-unreadable");
        return failure;
    };
    let Ok(constraint) = ExecutionConstraintDescriptor::parse(&constraint_bytes) else {
        eprintln!("amiss-bootstrap: constraint-invalid");
        return failure;
    };
    let Ok(own_path) = env::current_exe() else {
        eprintln!("amiss-bootstrap: self-unreadable");
        return failure;
    };
    let Ok(own_bytes) = std::fs::read(&own_path) else {
        eprintln!("amiss-bootstrap: self-unreadable");
        return failure;
    };
    let Ok(action) = Repository::open(&parsed.action_repository, constraint.action_object_format)
    else {
        eprintln!("amiss-bootstrap: action-tree-unavailable");
        return failure;
    };

    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let validated = match validate(&action, &mut resources, &constraint, &own_bytes) {
        Ok(validated) => validated,
        Err(refusal) => {
            eprintln!("amiss-bootstrap: {}", reason(refusal));
            return failure;
        }
    };

    let sealed = match capture_requests(&parsed, &constraint) {
        Ok(sealed) => sealed,
        Err(reason) => {
            eprintln!("amiss-bootstrap: {reason}");
            return failure;
        }
    };
    if pre_acquired(&parsed.repository, &sealed.evaluation).is_err() {
        eprintln!("amiss-bootstrap: repository-not-pre-acquired");
        return failure;
    }

    run_engine(&parsed, &validated, sealed)
}

const fn reason(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::UnsupportedPlatform => "unsupported-platform",
        Refusal::ActionTree => "action-tree-mismatch",
        Refusal::ActionMetadata => "action-metadata-invalid",
        Refusal::PathNotRegularBlob => "path-not-regular-blob",
        Refusal::ManifestUnreadable => "manifest-unreadable",
        Refusal::ManifestDigest => "manifest-digest-mismatch",
        Refusal::DependencyLock => "dependency-lock-mismatch",
        Refusal::ArtifactSelection => "artifact-selection-failed",
        Refusal::RuntimeClosure => "runtime-closure-mismatch",
        Refusal::EngineDigest => "engine-digest-mismatch",
        Refusal::PlatformBinding => "platform-binding-mismatch",
        Refusal::BootstrapDigest => "bootstrap-digest-mismatch",
    }
}

struct Args {
    action_repository: PathBuf,
    repository: PathBuf,
    constraint: PathBuf,
    evaluation_request: PathBuf,
    snapshot_request: PathBuf,
    controls_request: PathBuf,
}

fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut action_repository: Option<PathBuf> = None;
    let mut repository: Option<PathBuf> = None;
    let mut constraint: Option<PathBuf> = None;
    let mut evaluation_request: Option<PathBuf> = None;
    let mut snapshot_request: Option<PathBuf> = None;
    let mut controls_request: Option<PathBuf> = None;
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
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(PathBuf::from(value));
    }
    Some(Args {
        action_repository: action_repository?,
        repository: repository?,
        constraint: constraint?,
        evaluation_request: evaluation_request?,
        snapshot_request: snapshot_request?,
        controls_request: controls_request?,
    })
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
) -> Result<SealedRun, &'static str> {
    let streams = RequestStreams {
        evaluation: read_bounded(&args.evaluation_request)
            .map_err(|_defect| "evaluation-request-unreadable")?,
        snapshot: read_bounded(&args.snapshot_request)
            .map_err(|_defect| "snapshot-request-unreadable")?,
        controls: read_bounded(&args.controls_request)
            .map_err(|_defect| "controls-request-unreadable")?,
    };
    let evaluation = EvaluationRequest::parse(&streams.evaluation)
        .map_err(|_defect| "evaluation-request-invalid")?;
    let snapshot =
        SnapshotRequest::parse(&streams.snapshot).map_err(|_defect| "snapshot-request-invalid")?;
    let controls =
        ControlsRequest::parse(&streams.controls).map_err(|_defect| "controls-request-invalid")?;
    let canonical_requests = evaluation.canonical_bytes().ok().as_deref()
        == Some(streams.evaluation.as_slice())
        && snapshot.canonical_bytes().ok().as_deref() == Some(streams.snapshot.as_slice())
        && controls.canonical_bytes().ok().as_deref() == Some(streams.controls.as_slice());
    if !canonical_requests {
        return Err("request-noncanonical");
    }
    let candidate = match (evaluation.mode, evaluation.candidate_commit.as_ref()) {
        (RequestMode::CommitPair, Some(candidate))
            if snapshot.materialization == RequestMode::CommitPair =>
        {
            candidate.clone()
        }
        (RequestMode::CommitPair | RequestMode::Index, None | Some(_)) => {
            return Err("request-mode-mismatch");
        }
    };
    let repository = sealed_identity(&evaluation)?;
    let supplied_constraint = controls
        .execution_constraint
        .as_ref()
        .ok_or("execution-constraint-absent")?;
    let embedded_constraint =
        ExecutionConstraintDescriptor::parse(&canonical(&supplied_constraint.value))
            .map_err(|_defect| "execution-constraint-invalid")?;
    if embedded_constraint.digest != supplied_constraint.expected_digest
        || embedded_constraint != *constraint
    {
        return Err("execution-constraint-mismatch");
    }
    let supplied_time = controls
        .trusted_time
        .as_ref()
        .ok_or("trusted-time-absent")?;
    let statement = TrustedTimeStatement::parse(&canonical(&supplied_time.value))
        .map_err(|_defect| "trusted-time-invalid")?;
    if statement.digest != supplied_time.expected_digest
        || statement.provider != supplied_time.provider
        || statement.provider_run_id != supplied_time.provider_run_id
        || statement.provider_run_attempt != supplied_time.provider_run_attempt
    {
        return Err("trusted-time-mismatch");
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

fn read_bounded(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(REQUEST_STREAM_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > REQUEST_STREAM_BYTES {
        return Err(std::io::Error::other("request stream too large"));
    }
    Ok(bytes)
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

/// Writes the verified engine bytes into a private directory, launches them
/// with an empty environment, holds them to the wall ceiling, and republishes
/// only an accepted envelope. The bytes come from the validated tree, never
/// from a worktree file, a `PATH` lookup, or the action's launcher.
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn run_engine(args: &Args, validated: &amiss_bootstrap::Validated, sealed: SealedRun) -> ExitCode {
    let failure = ExitCode::from(2);
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

    let Ok(private) = tempfile::TempDir::new() else {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    };
    let engine = private.path().join(PRIVATE_ENGINE_NAME);
    if std::fs::write(&engine, &validated.binary).is_err() || executable_bit(&engine).is_err() {
        eprintln!("amiss-bootstrap: private-storage-unavailable");
        return failure;
    }

    let launched = std::process::Command::new(&engine)
        .arg(SEALED_ENGINE_ARGUMENT)
        .current_dir(&args.repository)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let Ok(mut child) = launched else {
        eprintln!("amiss-bootstrap: engine-launch-failed");
        return failure;
    };
    let Ok((outcome, wire)) = collect(&mut child, sealed.streams) else {
        eprintln!("amiss-bootstrap: engine-launch-failed");
        return failure;
    };
    match settle(&outcome, &wire, &expectations) {
        Ok(class) => publish(&wire, class),
        Err(defect) => {
            eprintln!("amiss-bootstrap: {}", refused(defect));
            failure
        }
    }
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

/// The accepted envelope is republished byte for byte, and the wrapper exits
/// with the class the engine claimed and was held to.
fn publish(wire: &[u8], class: i64) -> ExitCode {
    let mut out = std::io::stdout().lock();
    if out.write_all(wire).is_err() || out.flush().is_err() {
        return ExitCode::from(2);
    }
    u8::try_from(class).map_or(ExitCode::from(2), ExitCode::from)
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
