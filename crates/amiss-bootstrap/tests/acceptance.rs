#![expect(
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use amiss_bootstrap::supervise::{
    AcceptanceDefect, Defect, Expectations, Supervised, accept, settle, supervise,
};
use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::report::PAYLOAD_SCHEMA;

/// The frozen dossier examples: the indented readable envelope and its exact
/// one-line `JCS(envelope) || LF` canonicalization.
fn dossier_example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/spec/examples")
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
    }
}

#[test]
fn the_indented_example_is_rejected_as_noncanonical() {
    let indented = dossier_example("scanner-report-v1.json");
    assert_eq!(
        accept(&indented, &foreign_expectations()),
        Err(AcceptanceDefect::Noncanonical),
        "a readable parsed-value example is not a valid emitted byte fixture"
    );
}

#[test]
fn the_canonical_golden_is_the_canonicalization_of_the_indented_value() {
    let indented = dossier_example("scanner-report-v1.json");
    let golden = dossier_example("scanner-report-v1.canonical.json");
    let parsed = parse(&indented).unwrap();
    let mut recanonicalized = canonical(&parsed);
    recanonicalized.push(b'\n');
    assert_eq!(
        recanonicalized, golden,
        "the smoke-checker equivalence holds under this serializer"
    );
}

#[test]
fn the_canonical_golden_clears_the_canonicality_gate() {
    let golden = dossier_example("scanner-report-v1.canonical.json");
    let defect = accept(&golden, &foreign_expectations()).unwrap_err();
    assert_ne!(
        defect,
        AcceptanceDefect::Noncanonical,
        "the exact one-line golden is canonical"
    );
    assert_eq!(
        defect,
        AcceptanceDefect::PayloadDigest,
        "the frozen example's digest lives in the research namespace"
    );
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
/// it. The dossier golden's payload is kept whole and re-digested into this
/// namespace, so the payload digest recomputes; the expectations are then read
/// back out of the identities that payload actually carries.
fn accepted_report() -> (Vec<u8>, Expectations) {
    let golden = dossier_example("scanner-report-v1.canonical.json");
    let envelope = parse(&golden).unwrap();
    let payload = member(&envelope, "payload").unwrap().clone();
    let schema = member(&envelope, "schema").unwrap().clone();
    let digest = hj(PAYLOAD_SCHEMA, &payload).to_string();

    let engine_digest = text(member(&payload, "engine").unwrap(), "engine_digest").unwrap();
    let evaluation = member(&payload, "evaluation").unwrap();
    let base_commit = text(member(evaluation, "base").unwrap(), "commit_oid").unwrap();
    let candidate_commit = text(member(evaluation, "candidate").unwrap(), "commit_oid");

    let rebuilt = Value::Object(vec![
        ("payload".to_owned(), payload),
        ("payload_digest".to_owned(), Value::String(digest)),
        ("schema".to_owned(), schema),
    ]);
    let mut wire = canonical(&rebuilt);
    wire.push(b'\n');
    (
        wire,
        Expectations {
            engine_digest,
            base_commit,
            candidate_commit,
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
