use std::borrow::Cow;
use std::io::{BufWriter, Stdout, Write as _};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::report::{
    model::{RepoPath, ReportEnvelope, ReportEnvelopeSchema},
    validate_envelope,
};

use crate::invocation::{OutputFormat, RenderInvocation};

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &RenderInvocation, reserve: &mut BufWriter<Stdout>) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let input = match crate::input::strict_json(&invocation.report) {
        Ok(input) => input,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    let (payload, payload_digest, verdict) = match validate_envelope(&input.value) {
        Ok(validated) => validated,
        Err(defect) => {
            eprintln!("amiss render: {defect}");
            return failure;
        }
    };
    let result = if invocation.format == OutputFormat::Json {
        serde_json::from_slice::<serde_json::Value>(&input.bytes)
            .and_then(|value| serde_json_canonicalizer::to_writer(&value, reserve))
            .map_err(std::io::Error::from)
            .and_then(|()| {
                reserve.write_all(b"\n")?;
                reserve.flush()
            })
    } else {
        crate::project(
            &ReportEnvelope {
                payload,
                payload_digest,
                schema: ReportEnvelopeSchema::Current,
            },
            invocation.format,
            false,
            invocation.full,
            reserve,
            |path| match path {
                RepoPath::Text(path) => Ok(path.as_str()),
                RepoPath::Bytes(path) => Err(Cow::Borrowed(&path.bytes_hex)),
            },
        )
    };
    crate::projection_exit(result, ExitCode::from(verdict.code()))
}
