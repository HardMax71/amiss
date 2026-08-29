mod tests;

use amiss_wire::report::MACHINE_JSON_BYTES;

use super::ledger::{CheckConclusion, Publication};
use super::model::{
    ChangeSnapshot, ChangeState, Evaluation, RunFailure, RunIdentity, RunRequest, RunnerOutcome,
};

pub(super) struct PreparedPublication {
    pub(super) publication: Publication,
    pub(super) semantic_artifact: Option<Vec<u8>>,
}

pub(super) fn publication(
    request: &RunRequest,
    initial: &ChangeSnapshot,
    mut outcome: Option<RunnerOutcome>,
) -> PreparedPublication {
    let semantic_artifact = outcome.as_mut().and_then(|outcome| {
        if let RunnerOutcome::Complete {
            semantic_artifact, ..
        } = outcome
        {
            semantic_artifact.take()
        } else {
            None
        }
    });
    let (conclusion, report) = runner_conclusion(&initial.run, outcome);
    let semantic_artifact = report.as_ref().and(semantic_artifact);
    PreparedPublication {
        publication: Publication {
            provider_run: request.provider_run.clone(),
            evaluation_id: request.evaluation_id.clone(),
            check: request.check.clone(),
            run: initial.run.clone(),
            gate_commit: initial.gate_commit.clone(),
            conclusion,
            report,
            artifact: None,
        },
        semantic_artifact,
    }
}

pub(super) fn finalize_publication(
    initial: &ChangeSnapshot,
    fresh: &ChangeSnapshot,
    mut publication: Publication,
) -> Publication {
    let invalidated = if fresh.state == ChangeState::AuthorizationRevoked
        || initial.state == ChangeState::AuthorizationRevoked
    {
        Some(CheckConclusion::Unavailable(
            RunFailure::AuthorizationRevoked,
        ))
    } else if fresh.state == ChangeState::Closed || initial.state == ChangeState::Closed {
        Some(CheckConclusion::Unavailable(RunFailure::Closed))
    } else if fresh.state == ChangeState::Superseded
        || initial.state == ChangeState::Superseded
        || initial.run != fresh.run
        || initial.gate_commit != fresh.gate_commit
    {
        Some(CheckConclusion::Superseded)
    } else {
        None
    };
    if let Some(conclusion) = invalidated {
        publication.conclusion = conclusion;
    }
    publication
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
        Some(RunnerOutcome::OversizedOutput) => (
            CheckConclusion::Unavailable(RunFailure::OversizedOutput),
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
