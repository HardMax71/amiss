use std::process::ExitCode;

use crate::input::ReadError;
use crate::invocation::RecordSetInvocation;

enum Failure {
    Read(ReadError),
    Contract(amiss_wire::de::Error),
    Write(std::io::Error),
}

#[expect(clippy::print_stderr, reason = "authoring refusal channel")]
pub(crate) fn run(invocation: &RecordSetInvocation) -> ExitCode {
    let result = crate::input::bounded_bytes(
        &invocation.input,
        amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES,
    )
    .map_err(Failure::Read)
    .and_then(|bytes| amiss_wire::semantic::record::parse_input(&bytes).map_err(Failure::Contract))
    .and_then(|input| amiss_wire::semantic::record::template(input).map_err(Failure::Contract))
    .and_then(|template| {
        crate::output::write_json(&amiss_wire::json::canonical(&template)).map_err(Failure::Write)
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Write(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(Failure::Read(ReadError::Unreadable)) => {
            eprintln!("amiss record-set: evidence is unreadable");
            ExitCode::FAILURE
        }
        Err(Failure::Read(ReadError::TooLarge)) => {
            eprintln!("amiss record-set: evidence exceeds the semantic evidence byte ceiling");
            ExitCode::FAILURE
        }
        Err(Failure::Contract(error)) => {
            eprintln!("amiss record-set: {error}");
            ExitCode::FAILURE
        }
        Err(Failure::Write(_error)) => {
            eprintln!("amiss record-set: stdout could not be written");
            ExitCode::FAILURE
        }
    }
}
