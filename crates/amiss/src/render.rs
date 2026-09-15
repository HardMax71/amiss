use std::borrow::Cow;
use std::io::{BufWriter, Stdout};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::envelope::Payload as _;
use amiss_wire::report::model::{RepoPath, ReportPayload};
use amiss_wire::report::result_verdict;

use crate::invocation::RenderInvocation;

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &RenderInvocation, reserve: &mut BufWriter<Stdout>) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let input = match crate::input::report_bytes(&invocation.report) {
        Ok(input) => input,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    let validated = <ReportPayload>::parse(&input).and_then(|envelope| {
        result_verdict(&envelope.payload.result).map(|verdict| (envelope, verdict))
    });
    let (envelope, verdict) = match validated {
        Ok(validated) => validated,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    let result = crate::project(
        &envelope.payload,
        invocation.format,
        false,
        invocation.full,
        reserve,
        |out| amiss_wire::report::emit_report(&envelope, out),
        |path| match path {
            RepoPath::Text(path) => Ok(path.as_str()),
            RepoPath::Bytes(path) => Err(Cow::Borrowed(&path.bytes_hex)),
        },
    );
    crate::projection_exit(result, ExitCode::from(verdict.code()))
}
