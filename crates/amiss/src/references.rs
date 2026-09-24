use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::controls::MissingResolution;
use amiss_wire::envelope::Payload as _;
use amiss_wire::model::RepoPath;
use amiss_wire::report::ReportDefect;
use amiss_wire::report::model::{
    ObservedOccurrence, ReportPayload, ReportResolution, UnsupportedSemanticsResolution,
    occurrences,
};
use amiss_wire::resolution::{BlobTarget, Target, VersionScope};

use crate::invocation::{OutputFormat, RefsInvocation};

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
            if let Err(defect) = crate::output::write_json_array(&occurrences)
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

fn matching_occurrences(
    bytes: &[u8],
    target: &RepoPath,
) -> Result<Vec<ObservedOccurrence>, String> {
    let payload = <ReportPayload>::parse(bytes)
        .map_err(|error| error.to_string())?
        .payload;
    if !payload.result.complete {
        return Err(ReportDefect::Incomplete.to_string());
    }
    Ok(payload
        .observations
        .into_iter()
        .flat_map(|comparison| {
            occurrences(&comparison)
                .candidate
                .cloned()
                .into_iter()
                .chain(comparison.alternatives.candidate)
        })
        .filter(|occurrence| {
            let resolution_path = match &occurrence.resolution {
                ReportResolution::Resolved { target } | ReportResolution::TypeMismatch { target } => {
                    match target {
                        Target::Tree { path } | Target::Blob(BlobTarget { path, .. }) => Some(path),
                    }
                }
                ReportResolution::UnsupportedSemantics(UnsupportedSemanticsResolution {
                    target, ..
                }) => target.as_ref().map(|target| match target {
                    Target::Tree { path } | Target::Blob(BlobTarget { path, .. }) => path,
                }),
                ReportResolution::DeclaredUntracked { path, .. }
                | ReportResolution::UnsupportedTarget { path, .. }
                | ReportResolution::Missing(
                    MissingResolution::HeadingAnchorNotFound { path, .. }
                    | MissingResolution::LineFragmentOutOfRange { path }
                    | MissingResolution::PathNotFound { path, .. },
                )
                | ReportResolution::UnsupportedVersion {
                    scope: VersionScope::KnownPath { path } | VersionScope::KnownCommit { path, .. },
                } => Some(path),
                ReportResolution::External { .. }
                | ReportResolution::Invalid { .. }
                | ReportResolution::Missing(MissingResolution::LabelNotDeclared {})
                | ReportResolution::UnsupportedVersion { scope: VersionScope::UnknownPath {} } => None,
            };
            resolution_path
                .into_iter()
                .chain(
                    occurrence
                        .observation_id_input
                        .extracted_intent
                        .repository_path
                        .as_ref(),
                )
                .any(|path| path == target)
        })
        .collect())
}
