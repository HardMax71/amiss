use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::digest::Digest;
use amiss_wire::external::{parse_assessment, parse_plan};

use crate::invocation::{AssessInvocation, OutputFormat, PlanInvocation};

pub(crate) fn run_plan(invocation: &PlanInvocation) -> ExitCode {
    run_pure(
        "external-plan",
        invocation.format,
        || crate::input::report_bytes(&invocation.report),
        |report, version, digest| amiss_wire::external::plan(&report, version, digest),
        |bytes| {
            parse_plan(bytes)
                .map(|document| crate::human::plan(&document.payload))
                .map_err(|defect| defect.to_string())
        },
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation) -> ExitCode {
    run_pure(
        "external-assess",
        invocation.format,
        || {
            Ok((
                crate::input::report_bytes(&invocation.plan)?,
                crate::input::report_bytes(&invocation.evidence)?,
            ))
        },
        |(plan, evidence), version, digest| {
            amiss_wire::external::assess(&plan, &evidence, version, digest)
        },
        |bytes| {
            parse_assessment(bytes)
                .map(|document| crate::human::assessment(&document.payload))
                .map_err(|defect| defect.to_string())
        },
    )
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn run_pure<T, E: std::fmt::Display>(
    command: &str,
    format: OutputFormat,
    load: impl FnOnce() -> Result<T, String>,
    derive: impl FnOnce(T, &str, Digest) -> Result<Vec<u8>, E>,
    human: impl FnOnce(&[u8]) -> Result<(), String>,
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
        eprintln!(
            "amiss: {}",
            amiss_wire::report::AnalysisErrorCode::InternalError.as_ref()
        );
        return failure;
    };
    let bytes = match derive(input, &engine.version, engine.digest) {
        Ok(bytes) => bytes,
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            return failure;
        }
    };
    match format {
        OutputFormat::Json => {
            if let Err(defect) = crate::output::write_json(&bytes)
                && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss {command}: the artifact could not be written");
                return failure;
            }
        }
        OutputFormat::Human
        | OutputFormat::Sarif
        | OutputFormat::CodeQuality
        | OutputFormat::Junit => {
            if let Err(defect) = human(&bytes) {
                eprintln!("amiss {command}: {defect}");
                return failure;
            }
        }
    }
    ExitCode::from(ExitClass::Success.code())
}
