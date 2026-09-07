use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{
    MissingResolution, Occurrence, Resolution, UnsupportedSemanticsResolution,
};
use amiss_wire::report::{ReportDefect, validate_envelope};
use amiss_wire::resolution::{BlobTarget, TaggedBlobTarget, Target, VersionScope};

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

fn matching_occurrences(bytes: &[u8], target: &RepoPath) -> Result<Vec<Occurrence>, String> {
    let (report, _verdict) = validate_envelope(bytes).map_err(|error| error.to_string())?;
    if !report.payload.result.complete {
        return Err(ReportDefect::Incomplete.to_string());
    }
    let target_hex = hex::encode(target.as_bytes());
    Ok(report.payload
        .observations
        .into_iter()
        .flat_map(|comparison| {
            comparison
                .candidate
                .into_iter()
                .chain(comparison.alternatives.candidate)
        })
        .filter(|occurrence| {
            let resolution_path = match &occurrence.resolution {
                Resolution::Resolved { target }
                | Resolution::TypeMismatch { target }
                | Resolution::UnsupportedSemantics(
                    UnsupportedSemanticsResolution::Query { target }
                    | UnsupportedSemanticsResolution::CodeFragment { target },
                ) => match target {
                    Target::Tree { path } | Target::Blob(BlobTarget { path, .. }) => Some(path),
                },
                Resolution::DeclaredUntracked { path, .. }
                | Resolution::UnsupportedTarget { path, .. }
                | Resolution::Missing(
                    MissingResolution::HeadingAnchorNotFound { path, .. }
                    | MissingResolution::LineFragmentOutOfRange { path }
                    | MissingResolution::PathNotFound { path, .. },
                )
                | Resolution::UnsupportedVersion {
                    scope: VersionScope::KnownPath { path } | VersionScope::KnownCommit { path, .. },
                }
                | Resolution::UnsupportedSemantics(UnsupportedSemanticsResolution::Fragment {
                    target: TaggedBlobTarget::Blob(BlobTarget { path, .. }),
                }) => Some(path),
                Resolution::External { .. }
                | Resolution::Invalid { .. }
                | Resolution::Missing(MissingResolution::LabelNotDeclared {})
                | Resolution::UnsupportedVersion { scope: VersionScope::UnknownPath {} }
                | Resolution::UnsupportedSemantics(
                    UnsupportedSemanticsResolution::AttributeDependent {}
                    | UnsupportedSemanticsResolution::DuplicateLabel {}
                    | UnsupportedSemanticsResolution::ExternalInventory {}
                    | UnsupportedSemanticsResolution::NetworkPath {}
                    | UnsupportedSemanticsResolution::SiteRoute {},
                ) => None,
            };
            resolution_path
                .into_iter()
                .chain(occurrence.intent.repository_path.as_ref())
                .any(|path| match path {
                    amiss_wire::report::model::RepoPath::Text(path) => {
                        Some(path.as_str()) == target.as_str()
                    }
                    amiss_wire::report::model::RepoPath::Bytes(path) => {
                        target.as_str().is_none() && path.bytes_hex == target_hex
                    }
                })
        })
        .collect())
}
