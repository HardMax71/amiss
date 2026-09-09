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
use amiss_wire::controls::{Profile, parse_execution_constraint};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::MACHINE_JSON_BYTES;
use amiss_wire::report::model::{ReportEnvelope, ReportStatus};

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
    let execution = parse_execution_constraint(include_bytes!(
        "../../../spec/examples/scanner-execution-constraint.json"
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
            ProviderRunAttempt::try_from(1).unwrap(),
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
    report: &[u8],
) -> RunnerOutcome {
    classify_bootstrap_result(
        request,
        BootstrapTermination::Exited(exit_code),
        result.map(<[u8]>::to_vec),
        report.to_vec(),
        None,
    )
}

#[test]
fn pass_and_block_preserve_the_authenticated_run_and_report() {
    let request = request();
    let cases = [
        (
            BootstrapResult::Pass,
            0,
            Evaluation::Pass,
            ReportStatus::Pass,
        ),
        (
            BootstrapResult::Block,
            1,
            Evaluation::Block,
            ReportStatus::Fail,
        ),
    ];

    for (result, exit_code, evaluation, status) in cases {
        let mut envelope: ReportEnvelope =
            serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
        envelope.payload.result.status = status;
        envelope.payload.result.exit_code = exit_code;
        envelope.payload_digest = amiss_wire::digest::hb(
            amiss_wire::report::PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&envelope.payload).unwrap(),
        );
        let mut bytes = serde_json::to_vec_pretty(&envelope).unwrap();
        bytes.push(b'\n');
        assert_eq!(
            classify(
                &request,
                i32::from(exit_code),
                Some(result_bytes(result)),
                &bytes
            ),
            RunnerOutcome::Complete {
                identity: Box::new(request.run.clone()),
                evaluation,
                report: Arc::new(amiss_controller::CapturedReport { bytes, envelope }),
                semantic_artifact: None,
            }
        );
    }
}

#[test]
fn reports_require_a_complete_digest_true_envelope_and_matching_verdict() {
    let request = request();
    let report: ReportEnvelope = serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let text = std::str::from_utf8(amiss_fixtures::SCANNER_REPORT).unwrap();
    let engine = serde_json_canonicalizer::to_string(&report.payload.engine).unwrap();
    let pass = Some(result_bytes(BootstrapResult::Pass));
    for broken in [
        "not json".to_owned(),
        r#"{"payload":{"feedback":{"existing_count":0,"items":[],"status":"available"}}}"#
            .to_owned(),
        text.replace(&format!(r#""engine":{engine},"#), ""),
        text.replacen('{', r#"{"future":true,"#, 1),
        text.replacen('{', r#"{"duplicate":0,"duplicate":1,"#, 1),
        text.replace(
            &report.payload_digest.to_string(),
            &amiss_wire::digest::sha256(b"wrong payload").to_string(),
        ),
        text.replace(r#""existing_count":0"#, r#""existing_count":-1"#),
        format!("{text} null"),
    ] {
        assert_ne!(broken, text);
        assert_eq!(
            classify(&request, 0, pass, broken.as_bytes()),
            RunnerOutcome::TamperedRuntime
        );
    }
    let mut invalid_utf8 = amiss_fixtures::SCANNER_REPORT.to_vec();
    invalid_utf8.push(0xff);
    assert_eq!(
        classify(&request, 0, pass, &invalid_utf8),
        RunnerOutcome::TamperedRuntime
    );
    assert_eq!(
        classify(
            &request,
            1,
            Some(result_bytes(BootstrapResult::Block)),
            amiss_fixtures::SCANNER_REPORT
        ),
        RunnerOutcome::TamperedRuntime
    );

    for (status, complete, exit_code) in [
        (ReportStatus::Fail, true, 1),
        (ReportStatus::Incomplete, false, 2),
        (ReportStatus::Pass, true, 255),
    ] {
        let mut envelope = report.clone();
        envelope.payload.result.status = status;
        envelope.payload.result.complete = complete;
        envelope.payload.result.exit_code = exit_code;
        envelope.payload_digest = amiss_wire::digest::hb(
            amiss_wire::report::PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&envelope.payload).unwrap(),
        );
        assert_eq!(
            classify(&request, 0, pass, &serde_json::to_vec(&envelope).unwrap()),
            RunnerOutcome::TamperedRuntime
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
            Some(result_bytes(BootstrapResult::Pass).to_vec()),
            b"report".to_vec(),
            None,
        ),
        RunnerOutcome::TimedOut
    );
    assert_eq!(
        classify_bootstrap_result(
            &request,
            BootstrapTermination::TimedOut,
            None,
            Vec::new(),
            None,
        ),
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
                Some(result_bytes(BootstrapResult::Pass).to_vec()),
                b"report".to_vec(),
                None,
            ),
            RunnerOutcome::Unavailable
        );
    }
}
