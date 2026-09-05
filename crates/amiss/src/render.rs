use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::report::{FatalSerializer, validate_envelope};

use crate::invocation::RenderInvocation;

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &RenderInvocation, reserve: &mut FatalSerializer) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let envelope = match crate::input::strict_value(&invocation.report) {
        Ok(envelope) => envelope,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    let verdict = match validate_envelope(&envelope) {
        Ok((_payload, _digest, verdict)) => verdict,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    crate::projection_exit(
        crate::project(
            &envelope,
            invocation.format,
            false,
            invocation.full,
            reserve,
        ),
        ExitCode::from(verdict.code()),
    )
}
