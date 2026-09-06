use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{Occurrence, ReportEnvelope};
use amiss_wire::report::{ReportDefect, validate_envelope};
use serde::Deserialize;

use crate::invocation::{OutputFormat, RefsInvocation};

pub(crate) mod model;

use model::{Reference, ReferencePayload, ReferenceResolution};

#[expect(clippy::print_stderr, reason = "refusals are diagnostics")]
pub(crate) fn run(invocation: &RefsInvocation) -> ExitCode {
    let failure = ExitCode::from(ExitClass::Failure.code());
    let occurrences = match crate::input::report_bytes(&invocation.report)
        .and_then(|bytes| matching_occurrences(&bytes, &invocation.target))
    {
        Ok(occurrences) => occurrences,
        Err(defect) => {
            eprintln!("amiss refs: {defect}");
            return failure;
        }
    };
    match invocation.format {
        OutputFormat::Human => crate::human::references(&invocation.target, &occurrences),
        OutputFormat::Json => {
            // Each retained object occurs once in the bounded input.
            let original: Vec<_> = occurrences
                .iter()
                .map(|reference| &reference.original)
                .collect();
            if let Err(defect) = crate::output::write_json_array(&original)
                && defect.kind() != std::io::ErrorKind::BrokenPipe
            {
                eprintln!("amiss refs: the projection could not be written");
                return failure;
            }
        }
        OutputFormat::Sarif | OutputFormat::CodeQuality | OutputFormat::Junit => {}
    }
    ExitCode::from(ExitClass::Success.code())
}

fn matching_occurrences(bytes: &[u8], target: &RepoPath) -> Result<Vec<Reference>, String> {
    let (payload, _digest, _verdict) =
        validate_envelope(bytes).map_err(|error| error.to_string())?;
    if !payload.result.complete {
        return Err(ReportDefect::Incomplete.to_string());
    }
    drop(payload);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    // The shared report validator has already enforced the depth ceiling.
    deserializer.disable_recursion_limit();
    let envelope = ReportEnvelope::<ReferencePayload>::deserialize(&mut deserializer)
        .map_err(|defect| defect.to_string())?;
    let target_hex = hex::encode(target.as_bytes());
    envelope
        .payload
        .observations
        .into_iter()
        .flat_map(|comparison| {
            comparison
                .candidate
                .into_iter()
                .chain(comparison.alternatives.candidate)
        })
        .filter_map(|original| {
            Occurrence::<amiss_wire::report::model::RepoPath, ReferenceResolution>::deserialize(
                &original,
            )
            .map(|occurrence| {
                let resolution = &occurrence.resolution;
                let matches = [
                    occurrence.intent.repository_path.as_ref(),
                    resolution.path.as_ref(),
                    resolution
                        .target
                        .as_ref()
                        .and_then(|target| target.path.as_ref()),
                    resolution
                        .scope
                        .as_ref()
                        .and_then(|scope| scope.path.as_ref()),
                ]
                .into_iter()
                .flatten()
                .any(|path| match path {
                    amiss_wire::report::model::RepoPath::Text(path) => {
                        Some(path.as_str()) == target.as_str()
                    }
                    amiss_wire::report::model::RepoPath::Bytes(path) => {
                        target.as_str().is_none() && path.bytes_hex == target_hex
                    }
                });
                matches.then_some(Reference {
                    occurrence,
                    original,
                })
            })
            .transpose()
        })
        .collect::<Result<_, _>>()
        .map_err(|defect| defect.to_string())
}
