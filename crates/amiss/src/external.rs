use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::report::FatalSerializer;

use crate::invocation::{AssessInvocation, OutputFormat, PlanInvocation};

pub(crate) fn run_plan(invocation: &PlanInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    run_pure(
        "external-plan",
        invocation.format,
        reserve,
        || crate::input::strict_json(&invocation.report).map(|input| input.value),
        |report, version, digest| amiss_wire::external::plan(&report, version, digest),
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
                crate::input::strict_json(&invocation.plan)?.value,
                crate::input::strict_json(&invocation.evidence)?.bytes,
            ))
        },
        |(plan, evidence), version, digest| {
            amiss_wire::external::assess(&plan, &evidence, version, digest)
        },
        crate::human::assessment,
    )
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn run_pure<T, E: std::fmt::Display>(
    command: &str,
    format: OutputFormat,
    reserve: &mut FatalSerializer,
    load: impl FnOnce() -> Result<T, String>,
    derive: impl FnOnce(T, &str, Digest) -> Result<Value, E>,
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
    match derive(input, &engine.version, engine.digest) {
        Ok(envelope) => project(command, &envelope, format, reserve, human),
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            failure
        }
    }
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
        // The grammar admits human and json only; the other formats never reach here.
        OutputFormat::Human
        | OutputFormat::Sarif
        | OutputFormat::CodeQuality
        | OutputFormat::Junit => {
            human(envelope);
        }
    }
    ExitCode::from(ExitClass::Success.code())
}
