use amiss_wire::report::MACHINE_JSON_BYTES;

use super::ledger::{CheckConclusion, Publication};
use super::model::{
    ChangeSnapshot, ChangeState, Evaluation, RunFailure, RunIdentity, RunRequest, RunnerOutcome,
};

pub(super) fn publication(
    request: &RunRequest,
    initial: &ChangeSnapshot,
    fresh: &ChangeSnapshot,
    outcome: Option<RunnerOutcome>,
) -> Publication {
    let (conclusion, report) = if fresh.state == ChangeState::AuthorizationRevoked
        || initial.state == ChangeState::AuthorizationRevoked
    {
        (
            CheckConclusion::Unavailable(RunFailure::AuthorizationRevoked),
            None,
        )
    } else if fresh.state == ChangeState::Closed || initial.state == ChangeState::Closed {
        (CheckConclusion::Unavailable(RunFailure::Closed), None)
    } else if fresh.state == ChangeState::Superseded
        || initial.state == ChangeState::Superseded
        || initial.run != fresh.run
    {
        (CheckConclusion::Superseded, None)
    } else {
        runner_conclusion(&initial.run, outcome)
    };
    Publication {
        provider_run: request.provider_run.clone(),
        evaluation_id: request.evaluation_id.clone(),
        check: request.check.clone(),
        run: initial.run.clone(),
        conclusion,
        report,
    }
}

fn runner_conclusion(
    expected: &RunIdentity,
    outcome: Option<RunnerOutcome>,
) -> (CheckConclusion, Option<Vec<u8>>) {
    match outcome {
        Some(RunnerOutcome::Complete { identity, .. })
            if identity.change != expected.change
                || identity.refs != expected.refs
                || identity.object_format != expected.object_format
                || identity.commits != expected.commits =>
        {
            (
                CheckConclusion::Unavailable(RunFailure::WrongIdentity),
                None,
            )
        }
        Some(RunnerOutcome::Complete { identity, .. }) if identity.trees != expected.trees => {
            (CheckConclusion::Unavailable(RunFailure::WrongTree), None)
        }
        Some(RunnerOutcome::Complete { report, .. }) if report.is_empty() => (
            CheckConclusion::Unavailable(RunFailure::MissingOutput),
            None,
        ),
        Some(RunnerOutcome::Complete { report, .. })
            if u64::try_from(report.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES =>
        {
            (
                CheckConclusion::Unavailable(RunFailure::OversizedOutput),
                None,
            )
        }
        Some(RunnerOutcome::Complete {
            evaluation, report, ..
        }) => (
            match evaluation {
                Evaluation::Pass => CheckConclusion::Pass,
                Evaluation::Block => CheckConclusion::Block,
            },
            Some(report),
        ),
        Some(RunnerOutcome::MissingOutput) | None => (
            CheckConclusion::Unavailable(RunFailure::MissingOutput),
            None,
        ),
        Some(RunnerOutcome::TimedOut) => (CheckConclusion::Unavailable(RunFailure::Timeout), None),
        Some(RunnerOutcome::TamperedRuntime) => (
            CheckConclusion::Unavailable(RunFailure::TamperedRuntime),
            None,
        ),
        Some(RunnerOutcome::Unavailable) => {
            (CheckConclusion::Unavailable(RunFailure::Unavailable), None)
        }
    }
}
