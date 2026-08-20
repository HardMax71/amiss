use std::fs;
use std::io::Read as _;
use std::path::Path;
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::external::{AssessDefect, PlanDefect};
use amiss_wire::json::{self, Value};
use amiss_wire::report::{FatalSerializer, MACHINE_JSON_BYTES};

use crate::invocation::{AssessInvocation, OutputFormat, PlanInvocation};

pub(crate) fn run_plan(invocation: &PlanInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-plan",
        invocation.format,
        reserve,
        || strict_value(&invocation.report),
        |report, version, digest| {
            amiss_wire::external::plan(&report, version, digest).map_err(describe_plan)
        },
        crate::human::plan,
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-assess",
        invocation.format,
        reserve,
        || {
            Ok((
                strict_value(&invocation.plan)?,
                strict_value(&invocation.evidence)?,
            ))
        },
        |(plan, evidence), version, digest| {
            amiss_wire::external::assess(&plan, &evidence, version, digest).map_err(describe_assess)
        },
        crate::human::assessment,
    )
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn run_pure<T>(
    command: &str,
    format: OutputFormat,
    reserve: &mut FatalSerializer,
    load: impl FnOnce() -> Result<T, String>,
    derive: impl FnOnce(T, &str, &str) -> Result<Value, &'static str>,
    human: fn(&Value),
) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let input = match load() {
        Ok(input) => input,
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            return failure;
        }
    };
    let Some(engine) = crate::engine_provenance() else {
        return internal_error();
    };
    match derive(input, &engine.version, &engine.digest.to_string()) {
        Ok(envelope) => project(command, &envelope, format, reserve, human),
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            failure
        }
    }
}

/// The writer caps an envelope at `MACHINE_JSON_BYTES`, so a larger input
/// is provably not the scanner's artifact.
fn strict_value(path: &Path) -> Result<Value, String> {
    let shown = path.display();
    let file = fs::File::open(path).map_err(|_error| format!("{shown} is unreadable"))?;
    let mut bytes = Vec::new();
    file.take(MACHINE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_error| format!("{shown} is unreadable"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        return Err(format!("{shown} is larger than a scanner report can be"));
    }
    json::parse(&bytes).map_err(|_error| format!("{shown} is not the scanner's strict JSON"))
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn internal_error() -> ExitCode {
    eprintln!(
        "amiss: {}",
        amiss_wire::report::AnalysisErrorCode::InternalError.as_ref()
    );
    ExitCode::from(ExitClass::Failure.code())
}

/// A closed pipe never fails the exit; any other write defect lost bytes.
#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn project(
    command: &str,
    envelope: &Value,
    format: OutputFormat,
    reserve: &mut FatalSerializer,
    human: fn(&Value),
) -> ExitCode {
    match format {
        OutputFormat::Json => {
            if let Err(defect) = reserve.emit(envelope, &mut std::io::stdout())
                && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss {command}: the artifact could not be written");
                return ExitCode::from(ExitClass::Failure.code());
            }
        }
        // The grammar admits human and json only; the other two never reach here.
        OutputFormat::Human | OutputFormat::Sarif | OutputFormat::CodeQuality => {
            human(envelope);
        }
    }
    ExitCode::from(ExitClass::Success.code())
}

/// The command's own wording for each refusal; the wire enums stay data.
const fn describe_plan(defect: PlanDefect) -> &'static str {
    match defect {
        PlanDefect::NotAReport => "the input is not a scanner report envelope",
        PlanDefect::DigestMismatch => "the report payload does not match its recorded digest",
        PlanDefect::Incomplete => "the report is incomplete, so its sides cannot be compared",
        PlanDefect::MalformedExternal => {
            "an external occurrence is missing its destination, document, or scheme"
        }
    }
}

const fn describe_assess(defect: AssessDefect) -> &'static str {
    match defect {
        AssessDefect::NotAPlan => "the input is not an external plan envelope",
        AssessDefect::PlanDigestMismatch => "the plan payload does not match its recorded digest",
        AssessDefect::NotEvidence => "the input is not an external evidence file",
        AssessDefect::UnboundEvidence => {
            "the evidence binds another plan, repeats a destination, names one the plan did not \
             introduce, or resolves a tail the plan's shape does not carry"
        }
        AssessDefect::MalformedEvidence => "an evidence row breaks its own kind's grammar",
    }
}
