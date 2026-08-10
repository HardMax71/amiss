#![cfg(test)]

use std::sync::Arc;

use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::{
    ChangeId, ChangeLocator, ChangeSnapshot, ChangeState, CheckConclusion, DeliveryId,
    DeliveryIdentity, Evaluation, IntegrationId, OidPair, PolicyControls, ProviderIdentity,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunFailure, RunIdentity, RunRefs,
    RunnerOutcome, check_binding, check_plan,
};

use super::{publication, runner_conclusion};

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).expect("a sha1 oid")
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).expect("a branch ref")
}

fn run_identity(candidate: char) -> RunIdentity {
    let provider = ProviderIdentity {
        namespace: crate::ProviderNamespace::new("gitea".to_owned()).expect("a namespace"),
        instance: crate::ProviderInstance::new("forge.example".to_owned()).expect("an instance"),
    };
    let change = ChangeLocator {
        provider,
        repository: RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .expect("an identity"),
        change: ChangeId::new("pull/42".to_owned()).expect("a change"),
    };
    RunIdentity::new(
        change,
        RunRefs {
            forge: ForgeDialect::Gitea,
            candidate: branch("topic"),
            target: branch("main"),
            default_branch: branch("main"),
        },
        ObjectFormat::Sha1,
        OidPair {
            base: oid('a'),
            candidate: oid(candidate),
        },
        OidPair {
            base: oid('c'),
            candidate: oid('d'),
        },
    )
    .expect("a run identity")
}

fn request() -> super::super::model::RunRequest {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .expect("the published constraint");
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = oid('e');
    input.action_tree_oid = oid('f');
    let constraint = ExecutionConstraintDescriptor::new(input).expect("a constraint");
    let plan = Arc::new(
        check_plan(Profile::Enforce, PolicyControls::default(), constraint).expect("a plan"),
    );
    super::super::model::RunRequest {
        delivery: DeliveryIdentity {
            provider: ProviderIdentity {
                namespace: crate::ProviderNamespace::new("gitea".to_owned()).expect("a namespace"),
                instance: crate::ProviderInstance::new("forge.example".to_owned())
                    .expect("an instance"),
            },
            integration: IntegrationId::new("77".to_owned()).expect("an integration"),
            delivery: DeliveryId::new("signed-body".to_owned()).expect("a delivery"),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pr:run".to_owned()).expect("a run id"),
            ProviderRunAttempt::new(1).expect("an attempt"),
            ObjectFormat::Sha1,
            oid('b'),
        )
        .expect("a provider run"),
        evaluation_id: crate::ControllerEvaluationId::new("evaluation/1".to_owned())
            .expect("an evaluation id"),
        check: check_binding(&plan).expect("a binding"),
        plan,
        run: run_identity('b'),
    }
}

fn snapshot(state: ChangeState) -> ChangeSnapshot {
    ChangeSnapshot {
        state,
        run: run_identity('b'),
        gate_commit: oid('9'),
    }
}

/// Each disjunct of the publication ladder concludes on its own: either side
/// of the pair may have closed or moved, and the run or gate alone may drift.
#[test]
fn either_snapshot_alone_settles_the_conclusion() {
    let request = request();
    let active = snapshot(ChangeState::Active);
    let conclude = |initial: &ChangeSnapshot, fresh: &ChangeSnapshot| {
        let built = publication(&request, initial, fresh, None);
        assert!(built.report.is_none());
        built.conclusion
    };

    assert_eq!(
        conclude(&active, &snapshot(ChangeState::Closed)),
        CheckConclusion::Unavailable(RunFailure::Closed),
        "fresh alone closed"
    );
    assert_eq!(
        conclude(&snapshot(ChangeState::Closed), &active),
        CheckConclusion::Unavailable(RunFailure::Closed),
        "initial alone closed"
    );

    assert_eq!(
        conclude(&active, &snapshot(ChangeState::Superseded)),
        CheckConclusion::Superseded,
        "fresh alone superseded"
    );
    assert_eq!(
        conclude(&snapshot(ChangeState::Superseded), &active),
        CheckConclusion::Superseded,
        "initial alone superseded"
    );

    let mut moved = snapshot(ChangeState::Active);
    moved.run = run_identity('7');
    assert_eq!(
        conclude(&active, &moved),
        CheckConclusion::Superseded,
        "the run alone moved"
    );

    let mut regated = snapshot(ChangeState::Active);
    regated.gate_commit = oid('8');
    assert_eq!(
        conclude(&active, &regated),
        CheckConclusion::Superseded,
        "the gate alone moved"
    );

    assert_eq!(
        conclude(&active, &active),
        CheckConclusion::Unavailable(RunFailure::MissingOutput),
        "nothing moved and no runner answered"
    );
}

/// A finished runner owes a report: none at all is missing output, and the
/// wire ceiling is a width a report may reach and not pass.
#[test]
fn a_report_may_fill_the_wire_and_not_pass_it() {
    let expected = run_identity('b');
    let complete = |report: Vec<u8>| RunnerOutcome::Complete {
        identity: Box::new(run_identity('b')),
        evaluation: Evaluation::Pass,
        report,
    };

    assert_eq!(
        runner_conclusion(&expected, Some(complete(Vec::new()))),
        (
            CheckConclusion::Unavailable(RunFailure::MissingOutput),
            None
        ),
        "an empty report is no report"
    );

    let ceiling = usize::try_from(MACHINE_JSON_BYTES).expect("the ceiling fits this host");
    let full = vec![b'x'; ceiling];
    let (conclusion, report) = runner_conclusion(&expected, Some(complete(full)));
    assert_eq!(conclusion, CheckConclusion::Pass);
    assert_eq!(report.map(|bytes| bytes.len()), Some(ceiling));

    assert_eq!(
        runner_conclusion(&expected, Some(complete(vec![b'x'; ceiling + 1]))),
        (
            CheckConclusion::Unavailable(RunFailure::OversizedOutput),
            None
        ),
        "one byte past the wire is not a report the check can carry"
    );
}
