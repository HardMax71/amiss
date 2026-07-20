use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use amiss_wire::controls::{ExecutionConstraintDescriptor, TrustedTimeStatement};
use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::model::RepositoryIdentity;
use amiss_wire::report::{ENVELOPE_SCHEMA, PAYLOAD_SCHEMA};
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;

/// The exact acceptance defect, most specific first in evaluation order. The
/// trusted wrapper publishes success only when acceptance returns no defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptanceDefect {
    /// The bytes are not one parsable envelope with the expected members.
    Shape,
    /// The bytes are not exactly `JCS(envelope) || LF`.
    Noncanonical,
    /// The payload-only digest does not recompute.
    PayloadDigest,
    /// The engine digest differs from the binary the wrapper validated.
    Engine,
    /// The evaluated base identity differs from the one requested.
    BaseIdentity,
    /// The evaluated candidate identity differs from the one requested.
    CandidateIdentity,
    /// The report does not bind the sealed refs and candidate identity.
    SealedIdentity,
    /// The report does not carry the exact sealed controls and provider run.
    SealedControls,
    /// The completeness flag disagrees with the exit class.
    Completeness,
    /// The finding count differs from the findings array length.
    FindingCount,
}

/// What the wrapper expects the accepted envelope to carry: the digest of the
/// binary it validated and launched, and the identities it asked that binary
/// to evaluate. A wrapper can only hold an engine to what it knows it
/// requested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expectations {
    pub engine_digest: String,
    pub base_commit: String,
    pub candidate_commit: Option<String>,
    pub sealed: Option<SealedExpectations>,
}

/// The independently captured provider and control context a sealed report
/// must reproduce before the bootstrap will publish it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedExpectations {
    pub profile: String,
    pub candidate_ref: String,
    pub target_ref: String,
    pub repository: RepositoryIdentity,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub candidate_identity_digest: String,
    pub organization_floor: Option<SealedControlExpectation>,
    pub debt_snapshot: Option<SealedControlExpectation>,
    pub waiver_bundle: Option<SealedControlExpectation>,
    pub execution_constraint: SealedControlExpectation,
    pub trusted_time_digest: String,
}

/// One exact externally authenticated control projection expected in the
/// report after the engine verifies its embedded semantic digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedControlExpectation {
    pub digest: String,
    pub trust_source: String,
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

fn text<'value>(value: &'value Value, key: &str) -> Option<&'value str> {
    match member(value, key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// The acceptance law: the wire is exactly `JCS(envelope) || LF`, the
/// payload-only digest recomputes, the engine digest equals the validated
/// binary's, the evaluated identities equal the ones requested, the
/// completeness flag agrees with the exit class, and the finding count equals
/// the findings array length. Text printed before a crash is never
/// interpreted as a result. Success returns the envelope's exit class, so the
/// wrapper can hold the engine process to it.
///
/// # Errors
///
/// The first applicable defect in the order above.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<i64, AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    let envelope = parse(trimmed).map_err(|_defect| AcceptanceDefect::Shape)?;
    if canonical(&envelope) != trimmed {
        return Err(AcceptanceDefect::Noncanonical);
    }
    if text(&envelope, "schema") != Some(ENVELOPE_SCHEMA) {
        return Err(AcceptanceDefect::Shape);
    }
    let payload = member(&envelope, "payload").ok_or(AcceptanceDefect::Shape)?;
    if text(payload, "schema") != Some(PAYLOAD_SCHEMA) {
        return Err(AcceptanceDefect::Shape);
    }
    let recorded = text(&envelope, "payload_digest").ok_or(AcceptanceDefect::Shape)?;
    if hj(PAYLOAD_SCHEMA, payload).to_string() != recorded {
        return Err(AcceptanceDefect::PayloadDigest);
    }
    let engine_row = member(payload, "engine").ok_or(AcceptanceDefect::Shape)?;
    if text(engine_row, "engine_digest") != Some(expectations.engine_digest.as_str()) {
        return Err(AcceptanceDefect::Engine);
    }
    let evaluation = member(payload, "evaluation").ok_or(AcceptanceDefect::Shape)?;
    let resolved = text(evaluation, "status") != Some("unavailable");
    if expectations.sealed.is_some() && !resolved {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    if resolved {
        let base = member(evaluation, "base").ok_or(AcceptanceDefect::Shape)?;
        if text(base, "commit_oid") != Some(expectations.base_commit.as_str()) {
            return Err(AcceptanceDefect::BaseIdentity);
        }
        let candidate = member(evaluation, "candidate").ok_or(AcceptanceDefect::Shape)?;
        if let Some(expected) = expectations.candidate_commit.as_deref()
            && (text(candidate, "kind") != Some("git-commit")
                || text(candidate, "commit_oid") != Some(expected))
        {
            return Err(AcceptanceDefect::CandidateIdentity);
        }
        if let Some(sealed) = &expectations.sealed {
            accept_sealed(payload, evaluation, sealed)?;
        }
    }
    let result = member(payload, "result").ok_or(AcceptanceDefect::Shape)?;
    let exit_code = match member(result, "exit_code") {
        Some(Value::Integer(code)) => *code,
        _ => return Err(AcceptanceDefect::Shape),
    };
    let complete = member(result, "complete") == Some(&Value::Bool(true));
    if complete != (exit_code == 0 || exit_code == 1) {
        return Err(AcceptanceDefect::Completeness);
    }
    let count = match member(result, "finding_count") {
        Some(Value::Integer(count)) => *count,
        _ => return Err(AcceptanceDefect::Shape),
    };
    let findings = match member(payload, "findings") {
        Some(Value::Array(rows)) => rows.len(),
        _ => return Err(AcceptanceDefect::Shape),
    };
    if i64::try_from(findings).map_err(|_defect| AcceptanceDefect::Shape)? != count {
        return Err(AcceptanceDefect::FindingCount);
    }
    Ok(exit_code)
}

fn accept_sealed(
    payload: &Value,
    evaluation: &Value,
    expected: &SealedExpectations,
) -> Result<(), AcceptanceDefect> {
    if text(evaluation, "candidate_ref") != Some(expected.candidate_ref.as_str())
        || text(evaluation, "target_ref") != Some(expected.target_ref.as_str())
        || member(evaluation, "trusted_time") != Some(&Value::Bool(true))
    {
        return Err(AcceptanceDefect::SealedIdentity);
    }
    let Value::Object(evaluation_members) = evaluation else {
        return Err(AcceptanceDefect::Shape);
    };
    let mut identity_members: Vec<(String, Value)> = evaluation_members
        .iter()
        .filter(|(name, _value)| name != "evaluation_instant" && name != "trusted_time")
        .cloned()
        .collect();
    identity_members.push((
        "schema".to_owned(),
        Value::String(CANDIDATE_IDENTITY_DOMAIN.to_owned()),
    ));
    let identity = Value::Object(identity_members);
    let identity_digest = hj(CANDIDATE_IDENTITY_DOMAIN, &identity).to_string();
    if identity_digest != expected.candidate_identity_digest {
        return Err(AcceptanceDefect::SealedIdentity);
    }

    let controls = member(payload, "controls").ok_or(AcceptanceDefect::SealedControls)?;
    if text(controls, "profile") != Some(expected.profile.as_str()) {
        return Err(AcceptanceDefect::SealedControls);
    }
    accept_optional_control(
        controls,
        "organization_floor",
        expected.organization_floor.as_ref(),
    )?;
    accept_optional_control(controls, "debt_snapshot", expected.debt_snapshot.as_ref())?;
    accept_optional_control(controls, "waiver_bundle", expected.waiver_bundle.as_ref())?;
    let constraint =
        member(controls, "execution_constraint").ok_or(AcceptanceDefect::SealedControls)?;
    let descriptor = member(constraint, "descriptor").ok_or(AcceptanceDefect::SealedControls)?;
    let descriptor = ExecutionConstraintDescriptor::parse(&canonical(descriptor))
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if text(constraint, "status") != Some("verified")
        || text(constraint, "descriptor_digest")
            != Some(expected.execution_constraint.digest.as_str())
        || text(constraint, "trust_source")
            != Some(expected.execution_constraint.trust_source.as_str())
        || descriptor.digest.to_string() != expected.execution_constraint.digest
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let trusted =
        member(controls, "trusted_time_source").ok_or(AcceptanceDefect::SealedControls)?;
    let statement = member(trusted, "statement").ok_or(AcceptanceDefect::SealedControls)?;
    let statement = TrustedTimeStatement::parse(&canonical(statement))
        .map_err(|_defect| AcceptanceDefect::SealedControls)?;
    if text(trusted, "status") != Some("verified")
        || text(trusted, "trust_source") != Some("external-required-check")
        || text(trusted, "statement_digest") != Some(expected.trusted_time_digest.as_str())
        || statement.digest.to_string() != expected.trusted_time_digest
        || statement.provider != expected.provider
        || statement.provider_run_id != expected.provider_run_id
        || statement.provider_run_attempt != expected.provider_run_attempt
        || statement.repository != expected.repository
        || statement.ref_name.as_str() != expected.target_ref
        || statement.candidate_identity_digest.to_string() != identity_digest
        || text(evaluation, "evaluation_instant") != Some(statement.evaluation_instant.as_str())
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    let sandbox = member(controls, "sandbox").ok_or(AcceptanceDefect::SealedControls)?;
    if text(sandbox, "assurance") != Some("self-asserted")
        || text(sandbox, "enforcement_source") != Some("local-process")
        || member(sandbox, "verification") != Some(&Value::Null)
    {
        return Err(AcceptanceDefect::SealedControls);
    }
    Ok(())
}

fn accept_optional_control(
    controls: &Value,
    name: &str,
    expected: Option<&SealedControlExpectation>,
) -> Result<(), AcceptanceDefect> {
    let control = member(controls, name).ok_or(AcceptanceDefect::SealedControls)?;
    let accepted = match expected {
        Some(expected) => {
            text(control, "status") == Some("verified")
                && text(control, "digest") == Some(expected.digest.as_str())
                && text(control, "trust_source") == Some(expected.trust_source.as_str())
        }
        None => {
            text(control, "status") == Some("none")
                && member(control, "digest") == Some(&Value::Null)
                && text(control, "trust_source") == Some("none")
        }
    };
    if accepted {
        Ok(())
    } else {
        Err(AcceptanceDefect::SealedControls)
    }
}

/// The watchdog outcome for one spawned engine process.
#[derive(Debug)]
pub enum Supervised {
    /// The engine exited on its own within the ceiling.
    Completed(ExitStatus),
    /// The ceiling passed; the engine was killed and reaped. A killed engine
    /// yields no accepted envelope.
    Killed,
}

/// The operational wall-time watchdog: polls the engine until it exits or the
/// ceiling passes, then kills and reaps it. The kill can never produce a
/// partial result whose presence depends on runner speed; the caller fails the
/// run without an envelope.
///
/// # Errors
///
/// Only `try_wait` failures; kill and reap errors after a timeout are
/// deliberately ignored because the outcome is already `Killed`.
pub fn supervise(child: &mut Child, ceiling: Duration) -> std::io::Result<Supervised> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Supervised::Completed(status));
        }
        if start.elapsed() >= ceiling {
            let _signalled = child.kill();
            let _reaped = child.wait();
            return Ok(Supervised::Killed);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Why a run produced no accepted result. Every one of these is a failed
/// required check, and none of them publishes an envelope: a report the
/// wrapper cannot accept is not a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defect {
    /// The engine outlived the wall ceiling and was killed.
    Killed,
    /// The engine died on a signal and carries no exit code.
    Signalled,
    /// The engine wrote more than the wire ceiling admits.
    Oversize,
    /// The engine's own exit code disagrees with the exit class it reported.
    ExitMismatch,
    /// The envelope failed the acceptance law.
    Acceptance(AcceptanceDefect),
}

/// The settlement law, over what the wrapper can observe of a finished engine:
/// its exit code and its complete stdout. An accepted envelope returns the
/// exit class the wrapper then exits with, and which the engine's own process
/// exit code must already equal. Nothing else is publishable.
///
/// # Errors
///
/// The defect that refused the result.
pub fn settle(
    outcome: &Supervised,
    stdout: &[u8],
    expectations: &Expectations,
) -> Result<i64, Defect> {
    let status = match *outcome {
        Supervised::Killed => return Err(Defect::Killed),
        Supervised::Completed(status) => status,
    };
    if u64::try_from(stdout.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Err(Defect::Oversize);
    }
    let code = status.code().ok_or(Defect::Signalled)?;
    let class = accept(stdout, expectations).map_err(Defect::Acceptance)?;
    if i64::from(code) != class {
        return Err(Defect::ExitMismatch);
    }
    Ok(class)
}
