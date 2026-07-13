use amiss_wire::digest::hj;
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::report::PAYLOAD_SCHEMA;

/// The exact acceptance defect, most specific first in evaluation order. A
/// wrapper publishes success only when acceptance returns no defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptanceDefect {
    /// The bytes are not one parsable envelope with the expected members.
    Shape,
    /// The bytes are not exactly `JCS(envelope) || LF`.
    Noncanonical,
    /// The payload-only digest does not recompute.
    PayloadDigest,
    /// The engine digest differs from the wrapper's own engine provenance.
    Engine,
    /// The evaluated base identity differs from the request.
    BaseIdentity,
    /// The evaluated candidate identity differs from the request.
    CandidateIdentity,
    /// The resolved floor digest differs from the supplied expected digest.
    FloorDigest,
    /// The completeness flag disagrees with the exit class.
    Completeness,
    /// The finding count differs from the findings array length.
    FindingCount,
}

/// What the wrapper expects the accepted envelope to carry, derived from its
/// own provenance and the verified requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expectations {
    pub engine_digest: String,
    pub base_commit: String,
    pub candidate_commit: Option<String>,
    pub floor_digest: Option<String>,
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
/// payload-only digest recomputes, the evaluated identities equal the
/// request, the engine digest equals the wrapper's own provenance, the floor
/// digest equals the supplied expected digest when resolved, the
/// completeness flag agrees with the exit class, and the finding count
/// equals the findings array length. Text printed before a crash is never
/// interpreted as a result.
///
/// # Errors
///
/// The first applicable defect in the order above.
pub fn accept(wire: &[u8], expectations: &Expectations) -> Result<(), AcceptanceDefect> {
    let trimmed = wire
        .strip_suffix(b"\n")
        .ok_or(AcceptanceDefect::Noncanonical)?;
    let envelope = parse(trimmed).map_err(|_defect| AcceptanceDefect::Shape)?;
    if canonical(&envelope) != trimmed {
        return Err(AcceptanceDefect::Noncanonical);
    }
    let payload = member(&envelope, "payload").ok_or(AcceptanceDefect::Shape)?;
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
    if resolved {
        let base = member(evaluation, "base").ok_or(AcceptanceDefect::Shape)?;
        if text(base, "commit_oid") != Some(expectations.base_commit.as_str()) {
            return Err(AcceptanceDefect::BaseIdentity);
        }
        let candidate = member(evaluation, "candidate").ok_or(AcceptanceDefect::Shape)?;
        if let (Some(expected), Some("git-commit")) = (
            expectations.candidate_commit.as_deref(),
            text(candidate, "kind"),
        ) && text(candidate, "commit_oid") != Some(expected)
        {
            return Err(AcceptanceDefect::CandidateIdentity);
        }
        let controls_row = member(payload, "controls").ok_or(AcceptanceDefect::Shape)?;
        let controls_resolved = text(controls_row, "status") != Some("unavailable");
        if controls_resolved && let Some(expected) = expectations.floor_digest.as_deref() {
            let floor_row =
                member(controls_row, "organization_floor").ok_or(AcceptanceDefect::Shape)?;
            if text(floor_row, "digest") != Some(expected) {
                return Err(AcceptanceDefect::FloorDigest);
            }
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
    Ok(())
}
