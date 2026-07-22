use amiss_bootstrap::result::{BootstrapResult, RESULT_BYTES, parse_result, result_exit_code};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::{Evaluation, RunRequest, RunnerOutcome};

type Classification<T> = Result<T, RunnerOutcome>;

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
    exit_code(termination)
        .and_then(|exit_code| result_record(result).map(|result| (exit_code, result)))
        .and_then(verify_exit_code)
        .and_then(classify_record)
        .and_then(|evaluation| complete(request, evaluation, report))
        .unwrap_or_else(std::convert::identity)
}

fn exit_code(termination: BootstrapTermination) -> Classification<i32> {
    match termination {
        BootstrapTermination::Exited(exit_code) => Ok(exit_code),
        BootstrapTermination::TimedOut => Err(RunnerOutcome::TimedOut),
        BootstrapTermination::HeartbeatStopped
        | BootstrapTermination::Signalled
        | BootstrapTermination::SpawnUnavailable => Err(RunnerOutcome::Unavailable),
    }
}

fn result_record(result: Option<Vec<u8>>) -> Classification<BootstrapResult> {
    bounded_nonempty(result, RESULT_BYTES, RunnerOutcome::TamperedRuntime)
        .and_then(|bytes| parse_result(&bytes).ok_or(RunnerOutcome::TamperedRuntime))
}

fn verify_exit_code(
    (exit_code, result): (i32, BootstrapResult),
) -> Classification<BootstrapResult> {
    (exit_code == result_exit_code(result))
        .then_some(result)
        .ok_or(RunnerOutcome::TamperedRuntime)
}

fn classify_record(result: BootstrapResult) -> Classification<Evaluation> {
    match result {
        BootstrapResult::Pass => Ok(Evaluation::Pass),
        BootstrapResult::Block => Ok(Evaluation::Block),
        BootstrapResult::MissingOutput => Err(RunnerOutcome::MissingOutput),
        BootstrapResult::Timeout => Err(RunnerOutcome::TimedOut),
        BootstrapResult::OversizedOutput => Err(RunnerOutcome::OversizedOutput),
        BootstrapResult::TamperedRuntime => Err(RunnerOutcome::TamperedRuntime),
        BootstrapResult::Unavailable => Err(RunnerOutcome::Unavailable),
    }
}

fn complete(
    request: &RunRequest,
    evaluation: Evaluation,
    report: Vec<u8>,
) -> Classification<RunnerOutcome> {
    bounded_nonempty(
        Some(report),
        MACHINE_JSON_BYTES,
        RunnerOutcome::OversizedOutput,
    )
    .map(|report| RunnerOutcome::Complete {
        identity: Box::new(request.run.clone()),
        evaluation,
        report,
    })
}

fn bounded_nonempty(
    bytes: Option<Vec<u8>>,
    limit: u64,
    oversized: RunnerOutcome,
) -> Classification<Vec<u8>> {
    let bytes = bytes
        .filter(|bytes| !bytes.is_empty())
        .ok_or(RunnerOutcome::MissingOutput)?;

    u64::try_from(bytes.len())
        .is_ok_and(|size| size <= limit)
        .then_some(bytes)
        .ok_or(oversized)
}
