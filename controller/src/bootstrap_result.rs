use amiss_bootstrap::result::{BootstrapResult, RESULT_BYTES, parse_result};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::{Evaluation, RunRequest, RunnerOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapTermination {
    Exited,
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
    exit_code: Option<i32>,
    result: Option<&[u8]>,
    stdout: &[u8],
) -> RunnerOutcome {
    match termination {
        BootstrapTermination::TimedOut => RunnerOutcome::TimedOut,
        BootstrapTermination::HeartbeatStopped
        | BootstrapTermination::Signalled
        | BootstrapTermination::SpawnUnavailable => RunnerOutcome::Unavailable,
        BootstrapTermination::Exited => classify_exit(request, exit_code, result, stdout),
    }
}

fn classify_exit(
    request: &RunRequest,
    exit_code: Option<i32>,
    result: Option<&[u8]>,
    stdout: &[u8],
) -> RunnerOutcome {
    match result {
        None | Some([]) => RunnerOutcome::MissingOutput,
        Some(bytes) if u64::try_from(bytes.len()).map_or(true, |size| size > RESULT_BYTES) => {
            RunnerOutcome::TamperedRuntime
        }
        Some(bytes) => parse_result(bytes).map_or(RunnerOutcome::TamperedRuntime, |result| {
            classify_record(request, exit_code, result, stdout)
        }),
    }
}

fn classify_record(
    request: &RunRequest,
    exit_code: Option<i32>,
    result: BootstrapResult,
    stdout: &[u8],
) -> RunnerOutcome {
    match exit_code {
        Some(code) if code == result_exit_code(result) => accepted_record(request, result, stdout),
        None | Some(_) => RunnerOutcome::TamperedRuntime,
    }
}

const fn result_exit_code(result: BootstrapResult) -> i32 {
    match result {
        BootstrapResult::Pass => 0,
        BootstrapResult::Block => 1,
        BootstrapResult::MissingOutput
        | BootstrapResult::Timeout
        | BootstrapResult::OversizedOutput
        | BootstrapResult::TamperedRuntime
        | BootstrapResult::Unavailable => 2,
    }
}

fn accepted_record(request: &RunRequest, result: BootstrapResult, stdout: &[u8]) -> RunnerOutcome {
    match result {
        BootstrapResult::Pass => complete(request, Evaluation::Pass, stdout),
        BootstrapResult::Block => complete(request, Evaluation::Block, stdout),
        BootstrapResult::MissingOutput => RunnerOutcome::MissingOutput,
        BootstrapResult::Timeout => RunnerOutcome::TimedOut,
        BootstrapResult::OversizedOutput => RunnerOutcome::OversizedOutput,
        BootstrapResult::TamperedRuntime => RunnerOutcome::TamperedRuntime,
        BootstrapResult::Unavailable => RunnerOutcome::Unavailable,
    }
}

fn complete(request: &RunRequest, evaluation: Evaluation, stdout: &[u8]) -> RunnerOutcome {
    match (
        stdout.is_empty(),
        u64::try_from(stdout.len()).is_ok_and(|size| size <= MACHINE_JSON_BYTES),
    ) {
        (true, true) => RunnerOutcome::MissingOutput,
        (false, true) => RunnerOutcome::Complete {
            identity: Box::new(request.run.clone()),
            evaluation,
            report: stdout.to_vec(),
        },
        (true | false, false) => RunnerOutcome::OversizedOutput,
    }
}
