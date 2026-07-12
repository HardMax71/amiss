use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write as _;
use std::process::ExitCode;

use amiss::invocation::{self, Code, Outcome, OutputFormat};
use amiss_wire::ExitClass;
use amiss_wire::digest::hb;
use amiss_wire::report::{self, AnalysisErrorCode, EngineProvenance};

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(ExitClass::Failure.code());
    match invocation::parse(&argv) {
        Outcome::MalformedOutputSelection => {
            eprint!("{}", invocation::MALFORMED_OUTPUT_LINE);
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Json,
            codes,
        } => {
            let Some(engine) = engine_provenance() else {
                eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
                return failure;
            };
            let codes: BTreeSet<AnalysisErrorCode> =
                codes.iter().map(|code| analysis_code(*code)).collect();
            let Some(wire) = report::invocation_failure_wire(&engine, &codes) else {
                eprintln!(
                    "amiss: {}",
                    AnalysisErrorCode::ReportConstructionFailed.as_str()
                );
                return failure;
            };
            if std::io::stdout().write_all(&wire).is_err() {
                eprintln!(
                    "amiss: {}",
                    AnalysisErrorCode::ReportConstructionFailed.as_str()
                );
            }
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Human,
            codes,
        } => {
            for code in &codes {
                eprintln!("amiss: {}", code.as_str());
            }
            failure
        }
        Outcome::Accepted(_) => {
            // Fail closed until the evaluator exists.
            eprintln!("amiss: analysis not implemented");
            failure
        }
    }
}

fn engine_provenance() -> Option<EngineProvenance> {
    let exe = env::current_exe().ok()?;
    let bytes = fs::read(exe).ok()?;
    Some(EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: hb(report::ENGINE_DOMAIN, &bytes),
    })
}

fn analysis_code(code: Code) -> AnalysisErrorCode {
    match code {
        Code::InvalidEvent => AnalysisErrorCode::InvalidEvent,
        Code::InvalidInvocation => AnalysisErrorCode::InvalidInvocation,
        Code::InvalidProfile => AnalysisErrorCode::InvalidProfile,
        Code::UnsupportedProviderHost => AnalysisErrorCode::UnsupportedProviderHost,
    }
}
