mod engine;
mod invocation;
mod sealed;

use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use amiss_bootstrap::result::{BootstrapResult, result_bytes};
use amiss_bootstrap::supervise::{Defect, SealedExpectations};
use amiss_bootstrap::{Refusal, validate};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::parse_execution_constraint;
use amiss_wire::requests::{EvaluationRequest, RequestStreams};

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
/// `amiss-bootstrap --version`
#[expect(clippy::print_stderr, reason = "the bootstrap's diagnostic channel")]
fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    if let [only] = argv.as_slice()
        && only.to_str() == Some("--version")
    {
        let _ignored = writeln!(
            std::io::stdout(),
            "amiss-bootstrap {}",
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::SUCCESS;
    }
    let Some((parsed, mut output)) = invocation::parse_args(&argv).and_then(|parsed| {
        invocation::open_output(&parsed)
            .ok()
            .map(|output| (parsed, output))
    }) else {
        eprintln!("amiss-bootstrap: invalid-invocation");
        return ExitCode::from(2);
    };
    let completion = execute(&parsed)
        .and_then(|accepted| publish(&mut output.report, accepted))
        .unwrap_or_else(failed_completion);
    if let Some(diagnostic) = completion.diagnostic {
        eprintln!("amiss-bootstrap: {diagnostic}");
    }
    if write_output(&mut output.result, result_bytes(completion.result)).is_err() {
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

fn failed_completion(failure: Failure) -> Completion {
    Completion {
        result: failure.result,
        exit: ExitCode::from(2),
        diagnostic: Some(failure.diagnostic),
    }
}

fn execute(args: &Args) -> Execution<Accepted> {
    let constraint_bytes = sealed::read_input(
        &args.constraint,
        "constraint-unreadable",
        "constraint-invalid",
    )?;
    let constraint = parse_execution_constraint(&constraint_bytes)
        .map_err(|_defect| tampered("constraint-invalid"))?;
    let own_path = env::current_exe().map_err(|_defect| unavailable("self-unreadable"))?;
    let own_bytes = std::fs::read(own_path).map_err(|_defect| unavailable("self-unreadable"))?;
    let action = Repository::open(&args.action_repository, constraint.action_object_format)
        .map_err(|_defect| unavailable("action-tree-unavailable"))?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let validated =
        validate(&action, &mut resources, &constraint, &own_bytes).map_err(validation_failure)?;
    let sealed_run = sealed::capture_requests(args, &constraint)?;
    sealed::pre_acquired(&args.repository, &sealed_run.evaluation)
        .map_err(|()| unavailable("repository-not-pre-acquired"))?;
    engine::run(args, &validated, sealed_run)
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

struct OutputFiles {
    report: File,
    result: File,
}

#[derive(Clone)]
struct SealedRun {
    streams: RequestStreams,
    evaluation: EvaluationRequest,
    expected: SealedExpectations,
}

/// Publishes the accepted envelope before exposing its result record.
fn publish(report: &mut File, accepted: Accepted) -> Execution<Completion> {
    let Accepted {
        wire,
        class,
        result,
    } = accepted;
    write_output(report, &wire).map_err(|_defect| unavailable("report-publish-failed"))?;
    Ok(Completion {
        result,
        exit: ExitCode::from(class),
        diagnostic: None,
    })
}

fn write_output(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.flush()
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
        Defect::Acceptance(_defect) => tampered("report-rejected"),
    }
}
