#![expect(
    clippy::expect_used,
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
use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};
use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;

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
            .to_owned(),
        base_commit: "0000000000000000000000000000000000000000".to_owned(),
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
    let envelope = parse(&wire).unwrap();
    let payload = member(&envelope, "payload").unwrap();

    let engine_digest = text(member(payload, "engine").unwrap(), "engine_digest").unwrap();
    let evaluation = member(payload, "evaluation").unwrap();
    let base_commit = text(member(evaluation, "base").unwrap(), "commit_oid").unwrap();
    let candidate_commit = text(member(evaluation, "candidate").unwrap(), "commit_oid");

    (
        wire,
        Expectations {
            engine_digest,
            base_commit,
            candidate_commit,
            sealed: None,
        },
    )
}

fn member<'value>(value: &'value Value, key: &str) -> Option<&'value Value> {
    match value {
        Value::Object(members) => members
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, member)| member),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Array(_) => {
            None
        }
    }
}

fn text(value: &Value, key: &str) -> Option<String> {
    match member(value, key) {
        Some(Value::String(text)) => Some(text.clone()),
        _ => None,
    }
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
    let parsed = parse(&indented).unwrap();
    let mut recanonicalized = canonical(&parsed);
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
    let (wire, expectations) = sealed_report();
    assert_eq!(accept(&wire, &expectations), Ok(0));

    let wrong_ref = rewrite(&wire, |payload| {
        let evaluation = member_mut(payload, "evaluation");
        set_member(
            evaluation,
            "target_ref",
            Value::String("refs/heads/other".to_owned()),
        );
    });
    assert_eq!(
        accept(&wrong_ref, &expectations),
        Err(AcceptanceDefect::SealedIdentity)
    );

    let wrong_provider = rewrite(&wire, |payload| {
        let controls = member_mut(payload, "controls");
        let trusted = member_mut(controls, "trusted_time_source");
        let statement = member_mut(trusted, "statement");
        set_member(statement, "provider", Value::String("github".to_owned()));
    });
    assert_eq!(
        accept(&wrong_provider, &expectations),
        Err(AcceptanceDefect::SealedControls)
    );

    let wrong_profile = rewrite(&wire, |payload| {
        let controls = member_mut(payload, "controls");
        set_member(controls, "profile", Value::String("enforce".to_owned()));
    });
    assert_eq!(
        accept(&wrong_profile, &expectations),
        Err(AcceptanceDefect::SealedControls)
    );

    let dropped_floor = rewrite(&wire, |payload| {
        let controls = member_mut(payload, "controls");
        let floor = member_mut(controls, "organization_floor");
        set_member(floor, "status", Value::String("none".to_owned()));
    });
    assert_eq!(
        accept(&dropped_floor, &expectations),
        Err(AcceptanceDefect::SealedControls)
    );

    let changed_descriptor = rewrite(&wire, |payload| {
        let controls = member_mut(payload, "controls");
        let constraint = member_mut(controls, "execution_constraint");
        let descriptor = member_mut(constraint, "descriptor");
        set_member(
            descriptor,
            "required_status_name",
            Value::String("amiss / changed".to_owned()),
        );
    });
    assert_eq!(
        accept(&changed_descriptor, &expectations),
        Err(AcceptanceDefect::SealedControls)
    );

    let unavailable_hybrid = rewrite(&wire, |payload| {
        insert_member(
            member_mut(payload, "evaluation"),
            "status",
            Value::String("unavailable".to_owned()),
        );
    });
    assert_eq!(
        accept(&unavailable_hybrid, &expectations),
        Err(AcceptanceDefect::SealedIdentity)
    );
}

const FLOOR_DIGEST: &str =
    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn sealed_report() -> (Vec<u8>, Expectations) {
    let (wire, mut expectations) = accepted_report();
    let descriptor = parse(&dossier_example("scanner-execution-constraint.json")).unwrap();
    let constraint_digest = ExecutionConstraintDescriptor::parse(&canonical(&descriptor))
        .unwrap()
        .digest
        .to_string();
    let mut envelope = parse(&wire).unwrap();
    let payload = member_mut(&mut envelope, "payload");
    let evaluation = member_mut(payload, "evaluation");
    let candidate_identity_digest = seal_evaluation(evaluation);
    let (statement, time_digest) = sealed_statement(evaluation, &candidate_identity_digest);
    seal_controls(
        member_mut(payload, "controls"),
        descriptor,
        &constraint_digest,
        statement,
        &time_digest,
    );
    refresh_digest(&mut envelope);
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    expectations.sealed = Some(SealedExpectations {
        profile: "observe".to_owned(),
        candidate_ref: "refs/heads/feature/docs".to_owned(),
        target_ref: "refs/heads/main".to_owned(),
        provider: "gitlab".to_owned(),
        provider_run_id: "pipeline/42".to_owned(),
        provider_run_attempt: 2,
        candidate_identity_digest,
        organization_floor: Some(SealedControlExpectation {
            digest: FLOOR_DIGEST.to_owned(),
            trust_source: "organization-policy".to_owned(),
        }),
        debt_snapshot: None,
        waiver_bundle: None,
        execution_constraint: SealedControlExpectation {
            digest: constraint_digest,
            trust_source: "external-required-check".to_owned(),
        },
        trusted_time_digest: time_digest,
    });
    (wire, expectations)
}

fn seal_evaluation(evaluation: &mut Value) -> String {
    set_member(
        evaluation,
        "candidate_ref",
        Value::String("refs/heads/feature/docs".to_owned()),
    );
    set_member(
        evaluation,
        "target_ref",
        Value::String("refs/heads/main".to_owned()),
    );
    set_member(evaluation, "trusted_time", Value::Bool(true));
    set_member(
        evaluation,
        "evaluation_instant",
        Value::String("2026-07-12T10:00:00Z".to_owned()),
    );
    let Value::Object(members) = evaluation.clone() else {
        panic!("evaluation is an object");
    };
    let mut identity: Vec<(String, Value)> = members
        .into_iter()
        .filter(|(name, _value)| name != "evaluation_instant" && name != "trusted_time")
        .collect();
    identity.push((
        "schema".to_owned(),
        Value::String(CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    ));
    hj(CANDIDATE_IDENTITY_DOMAIN, &Value::Object(identity)).to_string()
}

fn sealed_statement(evaluation: &Value, identity_digest: &str) -> (Value, String) {
    let statement = object(vec![
        (
            "schema",
            Value::String("amiss/scanner-trusted-time-statement".to_owned()),
        ),
        (
            "controller",
            Value::String("external-required-check-clock".to_owned()),
        ),
        ("provider", Value::String("gitlab".to_owned())),
        (
            "repository",
            member(evaluation, "repository").unwrap().clone(),
        ),
        ("ref", Value::String("refs/heads/main".to_owned())),
        (
            "candidate_identity_digest",
            Value::String(identity_digest.to_owned()),
        ),
        ("provider_run_id", Value::String("pipeline/42".to_owned())),
        ("provider_run_attempt", Value::Integer(2)),
        (
            "evaluation_instant",
            Value::String("2026-07-12T10:00:00Z".to_owned()),
        ),
        (
            "valid_until",
            Value::String("2026-07-12T10:09:00Z".to_owned()),
        ),
    ]);
    let digest = TrustedTimeStatement::parse(&canonical(&statement))
        .unwrap()
        .digest
        .to_string();
    (statement, digest)
}

fn seal_controls(
    controls: &mut Value,
    descriptor: Value,
    constraint_digest: &str,
    statement: Value,
    time_digest: &str,
) {
    set_member(
        controls,
        "organization_floor",
        object(vec![
            ("status", Value::String("verified".to_owned())),
            ("digest", Value::String(FLOOR_DIGEST.to_owned())),
            (
                "trust_source",
                Value::String("organization-policy".to_owned()),
            ),
        ]),
    );
    set_member(
        controls,
        "execution_constraint",
        object(vec![
            ("status", Value::String("verified".to_owned())),
            (
                "descriptor_digest",
                Value::String(constraint_digest.to_owned()),
            ),
            ("descriptor", descriptor),
            (
                "trust_source",
                Value::String("external-required-check".to_owned()),
            ),
        ]),
    );
    set_member(
        controls,
        "trusted_time_source",
        object(vec![
            ("status", Value::String("verified".to_owned())),
            (
                "trust_source",
                Value::String("external-required-check".to_owned()),
            ),
            ("statement_digest", Value::String(time_digest.to_owned())),
            ("statement", statement),
        ]),
    );
}

fn rewrite(wire: &[u8], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut envelope = parse(wire).unwrap();
    edit(member_mut(&mut envelope, "payload"));
    refresh_digest(&mut envelope);
    let mut rewritten = canonical(&envelope);
    rewritten.push(b'\n');
    rewritten
}

fn refresh_digest(envelope: &mut Value) {
    let digest = hj(PAYLOAD_SCHEMA, member(envelope, "payload").unwrap()).to_string();
    set_member(envelope, "payload_digest", Value::String(digest));
}

fn member_mut<'value>(value: &'value mut Value, key: &str) -> &'value mut Value {
    let Value::Object(members) = value else {
        panic!("value is an object");
    };
    &mut members
        .iter_mut()
        .find(|(name, _value)| name == key)
        .expect("member exists")
        .1
}

fn set_member(value: &mut Value, key: &str, replacement: Value) {
    *member_mut(value, key) = replacement;
}

fn insert_member(value: &mut Value, key: &str, member: Value) {
    let Value::Object(members) = value else {
        panic!("value is an object");
    };
    assert!(members.iter().all(|(name, _value)| name != key));
    members.push((key.to_owned(), member));
}

fn object(rows: Vec<(&str, Value)>) -> Value {
    Value::Object(
        rows.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}
