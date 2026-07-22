use amiss_bootstrap::result::{BootstrapResult, RESULT_BYTES, parse_result, result_exit_code};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::{Evaluation, RunRequest, RunnerOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapTermination {
    Exited(i32),
    TimedOut,
    HeartbeatStopped,
    Signalled,
    SpawnUnavailable,
}

/// Classifies the closed result channel and captured report from one bootstrap
/// process. Outer supervision failures take precedence over process output.
#[must_use]
pub fn classify_bootstrap_result(
    request: &RunRequest,
    termination: BootstrapTermination,
    result: Option<Vec<u8>>,
    report: Vec<u8>,
) -> RunnerOutcome {
    match termination {
        BootstrapTermination::TimedOut => RunnerOutcome::TimedOut,
        BootstrapTermination::HeartbeatStopped
        | BootstrapTermination::Signalled
        | BootstrapTermination::SpawnUnavailable => RunnerOutcome::Unavailable,
        BootstrapTermination::Exited(exit_code) => {
            classify_exit(request, exit_code, result, report)
        }
    }
}

fn classify_exit(
    request: &RunRequest,
    exit_code: i32,
    result: Option<Vec<u8>>,
    report: Vec<u8>,
) -> RunnerOutcome {
    match result {
        None => RunnerOutcome::MissingOutput,
        Some(bytes) if bytes.is_empty() => RunnerOutcome::MissingOutput,
        Some(bytes) if u64::try_from(bytes.len()).map_or(true, |size| size > RESULT_BYTES) => {
            RunnerOutcome::TamperedRuntime
        }
        Some(bytes) => match parse_result(&bytes) {
            Some(result) => classify_record(request, exit_code, result, report),
            None => RunnerOutcome::TamperedRuntime,
        },
    }
}

fn classify_record(
    request: &RunRequest,
    exit_code: i32,
    result: BootstrapResult,
    report: Vec<u8>,
) -> RunnerOutcome {
    if exit_code == result_exit_code(result) {
        accepted_record(request, result, report)
    } else {
        RunnerOutcome::TamperedRuntime
    }
}

fn accepted_record(
    request: &RunRequest,
    result: BootstrapResult,
    report: Vec<u8>,
) -> RunnerOutcome {
    match result {
        BootstrapResult::Pass => complete(request, Evaluation::Pass, report),
        BootstrapResult::Block => complete(request, Evaluation::Block, report),
        BootstrapResult::MissingOutput => RunnerOutcome::MissingOutput,
        BootstrapResult::Timeout => RunnerOutcome::TimedOut,
        BootstrapResult::OversizedOutput => RunnerOutcome::OversizedOutput,
        BootstrapResult::TamperedRuntime => RunnerOutcome::TamperedRuntime,
        BootstrapResult::Unavailable => RunnerOutcome::Unavailable,
    }
}

fn complete(request: &RunRequest, evaluation: Evaluation, report: Vec<u8>) -> RunnerOutcome {
    match (
        report.is_empty(),
        u64::try_from(report.len()).is_ok_and(|size| size <= MACHINE_JSON_BYTES),
    ) {
        (true, true) => RunnerOutcome::MissingOutput,
        (false, true) => RunnerOutcome::Complete {
            identity: Box::new(request.run.clone()),
            evaluation,
            report,
        },
        (true | false, false) => RunnerOutcome::OversizedOutput,
    }
}
