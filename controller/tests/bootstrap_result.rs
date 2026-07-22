#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid wire identities"
)]

use std::sync::Arc;

use amiss_bootstrap::result::{BootstrapResult, RESULT_BYTES, result_bytes};
use amiss_controller::{
    BootstrapTermination, ChangeId, ChangeLocator, CheckPlan, ControllerEvaluationId, DeliveryId,
    DeliveryIdentity, Evaluation, IntegrationId, OidPair, PolicyControls, ProviderIdentity,
    ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
    RunIdentity, RunRefs, RunRequest, RunnerOutcome, check_binding, check_plan,
    classify_bootstrap_result,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::MACHINE_JSON_BYTES;

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example.internal".to_owned()).unwrap(),
    }
}

fn plan() -> Arc<CheckPlan> {
    let execution = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap())
}

fn request() -> RunRequest {
    let provider = provider();
    let plan = plan();
    RunRequest {
        delivery: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("project-hook/7".to_owned()).unwrap(),
            delivery: DeliveryId::new("webhook/9".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pipeline/987654321:job-42".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid('3'),
        )
        .unwrap(),
        evaluation_id: ControllerEvaluationId::new("evaluation/11".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            ChangeLocator {
                provider,
                repository: RepositoryIdentity::new(
                    "gitlab.example.internal".to_owned(),
                    "platform/security".to_owned(),
                    "docs".to_owned(),
                )
                .unwrap(),
                change: ChangeId::new("merge-request/42".to_owned()).unwrap(),
            },
            RunRefs {
                forge: ForgeDialect::Gitlab,
                candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid('1'),
                candidate: oid('3'),
            },
            OidPair {
                base: oid('2'),
                candidate: oid('4'),
            },
        )
        .unwrap(),
    }
}

fn classify(
    request: &RunRequest,
    exit_code: i32,
    result: Option<&[u8]>,
    stdout: &[u8],
) -> RunnerOutcome {
    classify_bootstrap_result(
        request,
        BootstrapTermination::Exited(exit_code),
        result,
        stdout,
    )
}

#[test]
fn pass_and_block_preserve_the_authenticated_run_and_report() {
    let request = request();
    let report = br#"{"schema":"amiss/scanner-report-envelope"}"#;
    let cases = [
        (BootstrapResult::Pass, 0, Evaluation::Pass),
        (BootstrapResult::Block, 1, Evaluation::Block),
    ];

    for (result, exit_code, evaluation) in cases {
        assert_eq!(
            classify(&request, exit_code, Some(result_bytes(result)), report,),
            RunnerOutcome::Complete {
                identity: Box::new(request.run.clone()),
                evaluation,
                report: report.to_vec(),
            }
        );
    }
}

#[test]
fn explicit_failures_map_to_the_closed_runner_outcomes() {
    let request = request();
    let cases = [
        (BootstrapResult::MissingOutput, RunnerOutcome::MissingOutput),
        (BootstrapResult::Timeout, RunnerOutcome::TimedOut),
        (
            BootstrapResult::OversizedOutput,
            RunnerOutcome::OversizedOutput,
        ),
        (
            BootstrapResult::TamperedRuntime,
            RunnerOutcome::TamperedRuntime,
        ),
        (BootstrapResult::Unavailable, RunnerOutcome::Unavailable),
    ];

    for (result, expected) in cases {
        assert_eq!(
            classify(&request, 2, Some(result_bytes(result)), b"ignored",),
            expected
        );
    }
}

#[test]
fn every_result_rejects_a_wrong_exit_code() {
    let request = request();
    let cases = [
        (BootstrapResult::Pass, 0),
        (BootstrapResult::Block, 1),
        (BootstrapResult::MissingOutput, 2),
        (BootstrapResult::Timeout, 2),
        (BootstrapResult::OversizedOutput, 2),
        (BootstrapResult::TamperedRuntime, 2),
        (BootstrapResult::Unavailable, 2),
    ];

    for (result, expected_exit) in cases {
        for exit_code in [-1, 0, 1, 2]
            .into_iter()
            .filter(|exit_code| *exit_code != expected_exit)
        {
            assert_eq!(
                classify(&request, exit_code, Some(result_bytes(result)), b"report",),
                RunnerOutcome::TamperedRuntime,
                "{result:?} accepted {exit_code:?}"
            );
        }
    }
}

#[test]
fn absent_empty_malformed_and_oversized_records_fail_closed() {
    let request = request();
    let oversized = vec![b'x'; usize::try_from(RESULT_BYTES).unwrap() + 1];
    let cases = [
        (None, RunnerOutcome::MissingOutput),
        (Some(&[][..]), RunnerOutcome::MissingOutput),
        (
            Some(&b"amiss/bootstrap-result-v1 pass\r\n"[..]),
            RunnerOutcome::TamperedRuntime,
        ),
        (Some(oversized.as_slice()), RunnerOutcome::TamperedRuntime),
    ];

    for (result, expected) in cases {
        assert_eq!(classify(&request, 0, result, b"report"), expected);
    }
}

#[test]
fn reports_must_be_nonempty_and_within_the_machine_limit() {
    let request = request();
    let pass = Some(result_bytes(BootstrapResult::Pass));
    assert_eq!(
        classify(&request, 0, pass, b""),
        RunnerOutcome::MissingOutput
    );

    let oversized = vec![b'x'; usize::try_from(MACHINE_JSON_BYTES).unwrap() + 1];
    assert_eq!(
        classify(&request, 0, pass, &oversized),
        RunnerOutcome::OversizedOutput
    );
}

#[test]
fn timeout_dominates_every_process_observation() {
    let request = request();
    assert_eq!(
        classify_bootstrap_result(
            &request,
            BootstrapTermination::TimedOut,
            Some(result_bytes(BootstrapResult::Pass)),
            b"report",
        ),
        RunnerOutcome::TimedOut
    );
    assert_eq!(
        classify_bootstrap_result(&request, BootstrapTermination::TimedOut, None, b"",),
        RunnerOutcome::TimedOut
    );
}

#[test]
fn stopped_signalled_and_unspawned_processes_are_unavailable() {
    let request = request();
    let terminations = [
        BootstrapTermination::HeartbeatStopped,
        BootstrapTermination::Signalled,
        BootstrapTermination::SpawnUnavailable,
    ];

    for termination in terminations {
        assert_eq!(
            classify_bootstrap_result(
                &request,
                termination,
                Some(result_bytes(BootstrapResult::Pass)),
                b"report",
            ),
            RunnerOutcome::Unavailable
        );
    }
}
