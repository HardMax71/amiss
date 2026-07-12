use std::env;
use std::process::ExitCode;

use amiss::invocation::{self, Outcome};
use amiss_wire::ExitClass;

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    match invocation::parse(&argv) {
        Outcome::MalformedOutputSelection => {
            eprint!("{}", invocation::MALFORMED_OUTPUT_LINE);
        }
        Outcome::Rejected { codes, .. } => {
            for code in &codes {
                eprintln!("amiss: {}", code.as_str());
            }
        }
        Outcome::Accepted(_) => {
            // Fail closed until the evaluator exists.
            eprintln!("amiss: analysis not implemented");
        }
    }
    ExitCode::from(ExitClass::Failure.code())
}
