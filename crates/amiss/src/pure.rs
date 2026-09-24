use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::model::Digest;

use crate::invocation::OutputFormat;

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run<T, E: std::fmt::Display>(
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
            amiss_wire::report::model::AnalysisErrorCode::InternalError.as_ref()
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
