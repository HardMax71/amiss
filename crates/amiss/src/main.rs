use std::process::ExitCode;

use amiss_wire::ExitClass;

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    // Fail closed until the invocation grammar exists.
    eprintln!("amiss: analysis not implemented");
    ExitCode::from(ExitClass::Failure.code())
}
