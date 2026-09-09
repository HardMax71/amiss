#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, SealedControlExpectation, SealedExpectations,
    Supervised, accept, settle, supervise,
};
use amiss_wire::controls::{
    Profile, TRUSTED_TIME_STATEMENT_SCHEMA, TrustedTimeController, TrustedTimeSchema,
    TrustedTimeStatement, canonical_execution_constraint, canonical_trusted_time,
    parse_execution_constraint,
};
use amiss_wire::digest::{hb, hj_serde};
use amiss_wire::model::RepositoryIdentity;
use amiss_wire::report::{PAYLOAD_SCHEMA, model};
use amiss_wire::requests::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema, CandidateSnapshot, RequestTrust,
};

mod ingress;
mod reader;

/// The frozen dossier examples: the indented readable envelope and its exact
/// one-line `JCS(envelope) || LF` canonicalization.
fn dossier_example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join(name),
    )
    .unwrap()
}

fn foreign_expectations() -> Expectations {
    Expectations {
        engine_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .unwrap(),
        base_commit: "0000000000000000000000000000000000000000".parse().unwrap(),
        candidate_commit: None,
        sealed: None,
    }
}

/// A killed engine yields no accepted result, whatever it managed to print.
#[test]
fn a_killed_engine_settles_to_nothing() {
    let (wire, expectations) = accepted_report();
    assert_eq!(
        settle(&Supervised::Killed, &wire, &expectations),
        Err(Defect::Killed),
        "a report that arrives after the ceiling is not a report"
    );
}

/// An engine that dies on a signal carries no exit code at all, so there is
/// nothing to compare an accepted class against, and a report it managed to
/// print before the fault is not evidence that the run finished. This is the
/// crash arm of the no-accepted-result law, and it is the one arm no synthetic
/// status can honestly stand in for, so the child really does abort. Only unix
/// can reach it: a Windows process that faults still exits with a code.
#[cfg(unix)]
#[test]
fn an_engine_that_dies_on_a_signal_settles_to_nothing() {
    let (wire, expectations) = accepted_report();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("kill -ABRT $$")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a shell that aborts itself");
    let outcome = supervise(&mut child, Duration::from_secs(30)).unwrap();

    let Supervised::Completed(status) = outcome else {
        panic!("the child aborted well inside the ceiling; it was not killed by the watchdog");
    };
    assert_eq!(
        status.code(),
        None,
        "a process that died on a signal has no exit code to report"
    );
    assert_eq!(
        settle(&Supervised::Completed(status), &wire, &expectations),
        Err(Defect::Signalled),
        "a perfectly good envelope from a process that crashed is still not a result"
    );
}

/// Text printed before a crash is never read as a result.
#[test]
fn a_prefixed_envelope_is_never_a_result() {
    let (wire, expectations) = accepted_report();
    let mut noisy = b"engine: warming up\n".to_vec();
    noisy.extend_from_slice(&wire);
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &noisy, &expectations),
        Err(Defect::Acceptance(AcceptanceDefect::Shape)),
        "stdout is the envelope or it is nothing"
    );
}

/// The engine's own exit code must equal the class it reported. A report
/// claiming a clean run from a process that failed is refused.
#[test]
fn an_engine_that_contradicts_its_own_report_is_refused() {
    let (wire, expectations) = accepted_report();
    assert_eq!(
        accept(&wire, &expectations),
        Ok(0),
        "the fixture is one accepted clean run"
    );
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &wire, &expectations),
        Ok(0),
        "an engine that agrees with its own report is published"
    );
    assert_eq!(
        settle(&Supervised::Completed(exited(1)), &wire, &expectations),
        Err(Defect::ExitMismatch),
        "an engine whose exit code disagrees with its report is refused"
    );
}

/// A report longer than the wire ceiling is refused before it is parsed.
#[test]
fn an_oversize_report_is_refused() {
    let (mut wire, expectations) = accepted_report();
    let ceiling = usize::try_from(amiss_wire::report::MACHINE_JSON_BYTES).unwrap();
    wire.resize(ceiling.saturating_add(1), b' ');
    assert_eq!(
        settle(&Supervised::Completed(exited(0)), &wire, &expectations),
        Err(Defect::Oversize),
        "the wrapper never parses past the ceiling"
    );
}

/// The launch itself: a cleared environment, a piped stdout, and a program
/// that is not the engine. `amiss-manifest` is a real executable this package
/// already builds, so it stands in for an engine that prints no envelope.
#[test]
fn a_launched_program_that_prints_no_envelope_is_refused() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_amiss-manifest"))
        .arg("--not-a-flag")
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let outcome = supervise(&mut child, Duration::from_secs(30)).unwrap();
    assert!(
        matches!(outcome, Supervised::Completed(_)),
        "the stand-in exits on its own"
    );
    let mut wire = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read as _;
        out.read_to_end(&mut wire).unwrap();
    }
    assert!(
        matches!(
            settle(&outcome, &wire, &foreign_expectations()),
            Err(Defect::Acceptance(_))
        ),
        "a program that is not the engine cannot satisfy the acceptance law"
    );
}

/// An exit status carrying `code`, built the only way each platform allows.
#[cfg(unix)]
fn exited(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt as _;
    ExitStatus::from_raw(code << 8)
}

#[cfg(windows)]
fn exited(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt as _;
    ExitStatus::from_raw(code.unsigned_abs())
}

/// One envelope the acceptance law admits, with the expectations that admit
/// it. The rolling-contract golden is this engine's own output under this exact
/// payload domain, so its recorded digest already recomputes here and the wire
/// is admitted whole, no re-digest; the expectations are read back out of the
/// identities that payload carries.
fn accepted_report() -> (Vec<u8>, Expectations) {
    let wire = dossier_example("scanner-report.canonical.json");
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the committed report has a resolved evaluation");
    };
    let (
        model::BaseSnapshot::Git(base),
        model::Snapshot::Available(CandidateSnapshot::Git(candidate)),
    ) = (&evaluation.base, &evaluation.candidate)
    else {
        panic!("the committed report describes a commit pair");
    };
    (
        wire,
        Expectations {
            engine_digest: report.payload.engine.engine_digest,
            base_commit: base.commit_oid.clone(),
            candidate_commit: Some(candidate.commit_oid.clone()),
            sealed: None,
        },
    )
}

#[test]
fn the_indented_contract_example_is_rejected_as_noncanonical() {
    let indented = dossier_example("scanner-report.json");
    assert_eq!(
        accept(&indented, &foreign_expectations()),
        Err(AcceptanceDefect::Noncanonical),
        "a readable parsed-value example is not a valid emitted byte fixture"
    );
}

#[test]
fn the_contract_golden_is_the_canonicalization_of_its_indented_value() {
    let indented = dossier_example("scanner-report.json");
    let golden = dossier_example("scanner-report.canonical.json");
    let mut recanonicalized = amiss_fixtures::canonical_json(&indented).unwrap();
    recanonicalized.push(b'\n');
    assert_eq!(
        recanonicalized, golden,
        "the smoke-checker equivalence holds under this serializer"
    );
}

/// The rolling golden clears the end-to-end acceptance law. Its payload
/// digest recomputes in the active domain, and both explicit schema identities
/// agree with the wrapper before any engine or evaluation claim is accepted.
#[test]
fn the_contract_golden_clears_the_acceptance_law_end_to_end() {
    let (wire, expectations) = accepted_report();
    assert_eq!(
        accept(&wire, &expectations),
        Ok(0),
        "the engine-emitted golden is admissible whole, digest included"
    );
}

#[test]
fn schema_labels_are_part_of_the_acceptance_law() {
    let (wire, expectations) = accepted_report();
    let text = String::from_utf8(wire).unwrap();
    let wrong_envelope = text.replacen(
        "amiss/scanner-report-envelope",
        "amiss/not-the-scanner-report-envelope",
        1,
    );
    assert_eq!(
        accept(wrong_envelope.as_bytes(), &expectations),
        Err(AcceptanceDefect::Shape),
        "a payload cannot ride a different envelope label"
    );

    let wrong_payload = text.replacen(
        "amiss/scanner-report-payload",
        "amiss/not-the-scanner-report-payload",
        1,
    );
    assert_eq!(
        accept(wrong_payload.as_bytes(), &expectations),
        Err(AcceptanceDefect::Shape),
        "a different payload label cannot pass under the report digest domain"
    );
}

#[test]
fn sealed_acceptance_binds_refs_provider_controls_and_candidate_identity() {
    let (report, expectations) = sealed_report();
    let mut wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    wire.push(b'\n');
    assert_eq!(accept(&wire, &expectations), Ok(0));

    let wrong_ref = rewrite(report.clone(), |payload| {
        let model::Evaluation::Resolved(evaluation) = &mut payload.evaluation else {
            panic!("the sealed report has a resolved evaluation");
        };
        evaluation.target_ref = Some("refs/heads/other".parse().unwrap());
    });
    assert_eq!(
        accept(&wrong_ref, &expectations),
        Err(AcceptanceDefect::SealedIdentity)
    );

    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the sealed report has resolved controls");
    };
    let [
        mut wrong_provider,
        mut wrong_profile,
        mut dropped_floor,
        mut changed_descriptor,
    ] = std::array::from_fn(|_| controls.clone());

    let model::TrustedTimeProvenance::Verified(trusted) = &mut wrong_provider.trusted_time_source
    else {
        panic!("the sealed report has verified trusted time");
    };
    trusted.statement.provider = "github".to_owned();
    wrong_profile.profile = Profile::Enforce;
    dropped_floor.organization_floor.status = model::ControlStatus::None;
    let model::ExecutionConstraintProvenance::Verified(constraint) =
        &mut changed_descriptor.execution_constraint
    else {
        panic!("the sealed report has a verified execution constraint");
    };
    constraint.descriptor.required_status_name = "amiss / changed".to_owned();

    for controls in [
        wrong_provider,
        wrong_profile,
        dropped_floor,
        changed_descriptor,
    ] {
        let changed = rewrite(report.clone(), |payload| {
            payload.controls = model::Controls::Resolved(controls);
        });
        assert_eq!(
            accept(&changed, &expectations),
            Err(AcceptanceDefect::SealedControls)
        );
    }
}

#[test]
fn sealed_acceptance_rejects_an_unavailable_hybrid() {
    let (report, expectations) = sealed_report();
    let payload = serde_json_canonicalizer::to_string(&report.payload).unwrap();
    let evaluation = serde_json_canonicalizer::to_string(&report.payload.evaluation).unwrap();
    assert_eq!(payload.matches(&evaluation).count(), 1);
    let hybrid = evaluation.replacen('{', r#"{"status":"unavailable","#, 1);
    assert_ne!(hybrid, evaluation);
    let changed = payload.replacen(&evaluation, &hybrid, 1);
    let changed =
        String::from_utf8(amiss_fixtures::canonical_json(changed.as_bytes()).unwrap()).unwrap();
    let mut wire = serde_json_canonicalizer::to_string(&report).unwrap();
    wire.push('\n');
    let digest = report.payload_digest.to_string();
    assert_eq!(wire.matches(&payload).count(), 1);
    assert_eq!(wire.matches(&digest).count(), 1);
    let malformed = wire.replacen(&payload, &changed, 1).replacen(
        &digest,
        &hb(PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
        1,
    );
    assert_eq!(
        accept(malformed.as_bytes(), &expectations),
        Err(AcceptanceDefect::Shape)
    );
}

/// The statement keeps a coherent digest chain while claiming another
/// repository, so only the direct repository binding can refuse it.
#[test]
fn a_statement_issued_for_another_repository_is_refused() {
    let (report, mut expectations) = sealed_report();
    let foreign = rewrite(report, |payload| {
        let model::Controls::Resolved(controls) = &mut payload.controls else {
            panic!("the sealed report has resolved controls");
        };
        let model::TrustedTimeProvenance::Verified(trusted) = &mut controls.trusted_time_source
        else {
            panic!("the sealed report has verified trusted time");
        };
        trusted.statement.repository = RepositoryIdentity::new(
            trusted.statement.repository.host().to_owned(),
            trusted.statement.repository.owner().to_owned(),
            "other".to_owned(),
        )
        .unwrap();
        trusted.statement_digest = hj_serde(TRUSTED_TIME_STATEMENT_SCHEMA, |mut writer| {
            serde_json_canonicalizer::to_writer(&trusted.statement, &mut writer)
        })
        .unwrap();
        expectations.sealed.as_mut().unwrap().trusted_time_digest = trusted.statement_digest;
    });
    assert_eq!(
        accept(&foreign, &expectations),
        Err(AcceptanceDefect::SealedControls)
    );
}

const FLOOR_DIGEST: &str =
    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn sealed_fixture_keeps_its_original_identities() {
    let (report, expectations) = sealed_report();
    let mut wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    wire.push(b'\n');
    assert_eq!(accept(&wire, &expectations), Ok(0));
    assert_eq!(
        serde_json::from_slice::<model::ReportEnvelope>(&wire).unwrap(),
        report
    );
    assert_eq!(
        amiss_fixtures::canonical_json(&wire).unwrap(),
        serde_json_canonicalizer::to_vec(&report).unwrap()
    );
    let sealed = expectations.sealed.unwrap();
    assert_eq!(
        [
            hb("amiss/test-bootstrap-sealed-report", &wire),
            report.payload_digest,
            sealed.candidate_identity_digest,
            sealed.execution_constraint.digest,
            sealed.trusted_time_digest,
        ]
        .map(|digest| digest.to_string()),
        [
            "sha256:9993a32b5aaadfaaf3c23638be6ad18c637cb74cf6e98656bb98c6d97455884d",
            "sha256:4401872e85015bedb26063129b7eacc05a49cce96cd216ef5a27dd1a7e47ec8c",
            "sha256:4c1c92f6b5ea7354763f2b45837dcca1905901bb54130a8893aabc30b5f3ac6e",
            "sha256:4b889138e21a65bad37ba80f833dae0764e5f0be8dc10a8b9f77b3bcc7a8c417",
            "sha256:85d982f94712e35f98bcd64adb9b4269007fd77381812bb94c7abbed814904de",
        ]
    );
}

fn sealed_report() -> (model::ReportEnvelope, Expectations) {
    let (wire, mut expectations) = accepted_report();
    let constraint =
        parse_execution_constraint(&dossier_example("scanner-execution-constraint.json")).unwrap();
    let constraint_digest = canonical_execution_constraint(&constraint).unwrap().1;
    let mut report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let model::Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("the committed report has a resolved evaluation");
    };
    evaluation.candidate_ref = Some("refs/heads/feature/docs".parse().unwrap());
    evaluation.target_ref = Some("refs/heads/main".parse().unwrap());
    evaluation.trusted_time = true;
    evaluation.evaluation_instant = Some("2026-07-12T10:00:00Z".to_owned().try_into().unwrap());
    let preimage = model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    };
    let candidate_identity_digest = hj_serde(CANDIDATE_IDENTITY_DOMAIN, |mut writer| {
        serde_json_canonicalizer::to_writer(&preimage, &mut writer)
    })
    .unwrap();
    let statement = TrustedTimeStatement {
        schema: TrustedTimeSchema::Current,
        controller: TrustedTimeController::ExternalRequiredCheckClock,
        provider: "gitlab".to_owned(),
        repository: evaluation.repository.clone().unwrap(),
        ref_name: "refs/heads/main".parse().unwrap(),
        candidate_identity_digest,
        provider_run_id: "pipeline/42".to_owned(),
        provider_run_attempt: 2,
        evaluation_instant: evaluation.evaluation_instant.clone().unwrap(),
        valid_until: "2026-07-12T10:09:00Z".to_owned().try_into().unwrap(),
    };
    let time_digest = canonical_trusted_time(&statement).unwrap().1;
    let model::Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("the committed report has resolved controls");
    };
    controls.semantic_evidence = Some(Vec::new());
    controls.organization_floor = model::ControlProvenance {
        status: model::ControlStatus::Verified,
        digest: Some(FLOOR_DIGEST.parse().unwrap()),
        trust_source: model::ControlTrustSource::Verified(RequestTrust::OrganizationPolicy),
    };
    controls.execution_constraint = model::ExecutionConstraintProvenance::Verified(Box::new(
        model::VerifiedExecutionConstraint {
            status: model::VerifiedControlStatus::Verified,
            descriptor_digest: constraint_digest,
            descriptor: constraint,
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
    ));
    controls.trusted_time_source =
        model::TrustedTimeProvenance::Verified(Box::new(model::VerifiedTrustedTime {
            status: model::VerifiedControlStatus::Verified,
            trust_source: model::TrustedTimeTrustSource::ExternalRequiredCheck,
            statement_digest: time_digest,
            statement,
        }));
    report.payload_digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })
    .unwrap();
    expectations.sealed = Some(SealedExpectations {
        profile: Profile::Observe,
        candidate_ref: "refs/heads/feature/docs".to_owned(),
        target_ref: "refs/heads/main".to_owned(),
        repository: RepositoryIdentity::new(
            "git.example.internal".to_owned(),
            "group/subgroup".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
        provider: "gitlab".to_owned(),
        provider_run_id: "pipeline/42".to_owned(),
        provider_run_attempt: 2,
        candidate_identity_digest,
        organization_floor: Some(SealedControlExpectation {
            digest: FLOOR_DIGEST.parse().unwrap(),
            trust_source: RequestTrust::OrganizationPolicy,
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        execution_constraint: SealedControlExpectation {
            digest: constraint_digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        },
        trusted_time_digest: time_digest,
        semantic_evidence: Vec::new(),
    });
    (report, expectations)
}

fn rewrite(
    mut report: model::ReportEnvelope,
    edit: impl FnOnce(&mut model::ReportPayload),
) -> Vec<u8> {
    edit(&mut report.payload);
    report.payload_digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })
    .unwrap();
    let mut wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    wire.push(b'\n');
    wire
}
