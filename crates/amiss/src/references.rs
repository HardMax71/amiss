use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::json::{self, Value};
use amiss_wire::model::RepoPath;
use amiss_wire::report::{MACHINE_JSON_BYTES, ReportDefect, validate_envelope};

use crate::invocation::{OutputFormat, RefsInvocation};

const MALFORMED: &str = "the report carries a malformed candidate occurrence";

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &RefsInvocation) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let envelope = match crate::input::strict_value(&invocation.report) {
        Ok(envelope) => envelope,
        Err(defect) => {
            eprintln!("amiss refs: {defect}");
            return failure;
        }
    };
    let occurrences = match matching_occurrences(&envelope, &invocation.target) {
        Ok(occurrences) => occurrences,
        Err(defect) => {
            eprintln!("amiss refs: {defect}");
            return failure;
        }
    };
    match invocation.format {
        OutputFormat::Human => crate::human::references(&invocation.target, &occurrences),
        OutputFormat::Json => {
            if projected_bytes(&occurrences) > MACHINE_JSON_BYTES {
                eprintln!("amiss refs: the projection is larger than a scanner report can be");
                return failure;
            }
            if let Err(defect) = crate::output::write_json_array(&occurrences, |occurrence| {
                json::canonical(occurrence)
            }) && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss refs: the projection could not be written");
                return failure;
            }
        }
        OutputFormat::Sarif | OutputFormat::CodeQuality | OutputFormat::Junit => {}
    }
    ExitCode::from(ExitClass::Success.code())
}

fn matching_occurrences<'report>(
    envelope: &'report Value,
    target: &RepoPath,
) -> Result<Vec<&'report Value>, String> {
    let (payload, _digest, _verdict) =
        validate_envelope(envelope).map_err(|error| error.to_string())?;
    if payload
        .member("result")
        .and_then(|result| result.member("complete"))
        != Some(&Value::Bool(true))
    {
        return Err(ReportDefect::Incomplete.to_string());
    }
    let Some(Value::Array(comparisons)) = payload.member("observations") else {
        return Err(ReportDefect::NotAReport.to_string());
    };
    let target = target.to_value();
    let mut matched = Vec::new();
    for comparison in comparisons {
        let Some(candidate) = comparison.member("candidate") else {
            return Err(MALFORMED.to_owned());
        };
        match candidate {
            Value::Null => {}
            Value::Object(_) => retain(candidate, &target, &mut matched)?,
            Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Array(_) => {
                return Err(MALFORMED.to_owned());
            }
        }
        let Some(Value::Array(alternatives)) = comparison
            .member("alternatives")
            .and_then(|alternatives| alternatives.member("candidate"))
        else {
            return Err(MALFORMED.to_owned());
        };
        for alternative in alternatives {
            retain(alternative, &target, &mut matched)?;
        }
    }
    Ok(matched)
}

fn retain<'report>(
    occurrence: &'report Value,
    target: &Value,
    matched: &mut Vec<&'report Value>,
) -> Result<(), String> {
    if occurrence_matches(occurrence, target).map_err(str::to_owned)? {
        matched.push(occurrence);
    }
    Ok(())
}

fn occurrence_matches(occurrence: &Value, target: &Value) -> Result<bool, &'static str> {
    let intent = occurrence.member("intent").ok_or(MALFORMED)?;
    let resolution = occurrence.member("resolution").ok_or(MALFORMED)?;
    let repository_path = intent.member("repository_path").ok_or(MALFORMED)?;
    Ok([
        Some(repository_path),
        resolution.member("path"),
        resolution
            .member("target")
            .and_then(|value| value.member("path")),
        resolution
            .member("scope")
            .and_then(|value| value.member("path")),
    ]
    .into_iter()
    .flatten()
    .any(|path| path == target))
}

fn projected_bytes(occurrences: &[&Value]) -> u64 {
    let separators = u64::try_from(occurrences.len().saturating_sub(1)).unwrap_or(u64::MAX);
    occurrences
        .iter()
        .fold(3_u64.saturating_add(separators), |total, occurrence| {
            total.saturating_add(json::canonical_length(occurrence))
        })
}
