use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::digest::Digest;
use amiss_wire::external::EXTERNAL_DOCUMENT_BYTES;

use crate::invocation::{AssessInvocation, OutputFormat, PlanInvocation};

pub(crate) fn run_plan(invocation: &PlanInvocation) -> ExitCode {
    run_pure(
        "external-plan",
        invocation.format,
        || {
            let bytes = crate::input::report_bytes(&invocation.report)?;
            amiss_wire::report::validate_envelope(&bytes)
                .map(|(report, _verdict)| report)
                .map_err(|defect| defect.to_string())
        },
        |report, version, digest| amiss_wire::external::plan(&report, version, digest),
        |document| crate::human::plan(&document.payload),
    )
}

pub(crate) fn run_assess(invocation: &AssessInvocation) -> ExitCode {
    run_pure(
        "external-assess",
        invocation.format,
        || {
            let plan = crate::input::report_bytes(&invocation.plan)?;
            let evidence = crate::input::report_bytes(&invocation.evidence)?;
            let plan = amiss_wire::external::parse_plan(&plan)
                .map_err(|defect| amiss_wire::external::AssessDefect::Plan(defect).to_string())?;
            let (evidence, _) =
                amiss_wire::external::parse_evidence(&evidence).map_err(|defect| {
                    amiss_wire::external::AssessDefect::Evidence(defect).to_string()
                })?;
            Ok((plan, evidence))
        },
        |(plan, evidence), version, digest| {
            amiss_wire::external::assess(&plan, &evidence, version, digest)
        },
        |document| crate::human::assessment(&document.payload),
    )
}

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
fn run_pure<T, O: serde::Serialize, E: std::fmt::Display>(
    command: &str,
    format: OutputFormat,
    load: impl FnOnce() -> Result<T, String>,
    derive: impl FnOnce(T, &str, Digest) -> Result<O, E>,
    human: impl FnOnce(&O),
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
    let document = match derive(input, &engine.version, engine.digest) {
        Ok(document) => document,
        Err(defect) => {
            eprintln!("amiss {command}: {defect}");
            return failure;
        }
    };
    let mut bytes = Vec::new();
    let mut sink = std::io::sink();
    let output: &mut dyn std::io::Write = if format == OutputFormat::Json {
        &mut bytes
    } else {
        &mut sink
    };
    if let Err(defect) = amiss_wire::write_json(&document, output, EXTERNAL_DOCUMENT_BYTES) {
        eprintln!("amiss {command}: {defect}");
        return failure;
    }
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
            human(&document);
        }
    }
    ExitCode::from(ExitClass::Success.code())
}
