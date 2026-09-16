use std::borrow::Cow;
use std::io::{BufWriter, Stdout};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::envelope::Payload as _;
use amiss_wire::human::atom;
use amiss_wire::report::model::{
    MissingResolution, RepoPath, ReportPayload, Resolution as WireResolution,
    UnsupportedSemanticsResolution,
};
use amiss_wire::report::result_verdict;
use amiss_wire::resolution::{MissingTag, ResolutionTag, UnsupportedSemanticsTag, VersionScopeTag};

use crate::human::missing_detail;
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
        crate::human::Options {
            explain_scope: false,
            full: invocation.full,
        },
        reserve,
        |out| amiss_wire::report::emit_report(&envelope, out),
        |path| match path {
            RepoPath::Text(path) => Ok(path.as_str()),
            RepoPath::Bytes(path) => Err(Cow::Borrowed(&path.bytes_hex)),
        },
        |resolution| wire_resolution(resolution, crate::human::wire_path),
    );
    crate::projection_exit(result, ExitCode::from(verdict.code()))
}

/// A resolution read back from a report, spelled for a human place line over
/// the path form that report carries.
pub(crate) fn wire_resolution<P, F: Fn(&P) -> String>(
    resolution: &WireResolution<P>,
    path: F,
) -> (ResolutionTag, Option<String>) {
    match resolution {
        WireResolution::Missing(missing) => {
            let (tag, near) = match missing {
                MissingResolution::PathNotFound { near, .. } => {
                    (MissingTag::PathNotFound, near.as_ref().map(path))
                }
                MissingResolution::HeadingAnchorNotFound { near, .. } => (
                    MissingTag::HeadingAnchorNotFound,
                    near.as_ref().map(|near| atom(near)),
                ),
                MissingResolution::LineFragmentOutOfRange { .. } => {
                    (MissingTag::LineFragmentOutOfRange, None)
                }
                MissingResolution::LabelNotDeclared {} => (MissingTag::LabelNotDeclared, None),
            };
            (ResolutionTag::Missing, Some(missing_detail(tag, near)))
        }
        WireResolution::Invalid { reason } => {
            (ResolutionTag::Invalid, Some(reason.as_ref().to_owned()))
        }
        WireResolution::UnsupportedTarget { reason, .. } => (
            ResolutionTag::UnsupportedTarget,
            Some(reason.as_ref().to_owned()),
        ),
        WireResolution::UnsupportedSemantics(semantics) => (
            ResolutionTag::UnsupportedSemantics,
            Some(semantics_tag(semantics).as_ref().to_owned()),
        ),
        WireResolution::UnsupportedVersion { scope } => (
            ResolutionTag::UnsupportedVersion,
            Some(VersionScopeTag::from(scope).as_ref().to_owned()),
        ),
        WireResolution::Resolved { .. } => (ResolutionTag::Resolved, None),
        WireResolution::TypeMismatch { .. } => (ResolutionTag::TypeMismatch, None),
        WireResolution::DeclaredUntracked { .. } => (ResolutionTag::DeclaredUntracked, None),
        WireResolution::External { .. } => (ResolutionTag::External, None),
    }
}

const fn semantics_tag<P>(
    semantics: &UnsupportedSemanticsResolution<P>,
) -> UnsupportedSemanticsTag {
    match semantics {
        UnsupportedSemanticsResolution::AttributeDependent {} => {
            UnsupportedSemanticsTag::AttributeDependent
        }
        UnsupportedSemanticsResolution::CodeFragment { .. } => {
            UnsupportedSemanticsTag::CodeFragment
        }
        UnsupportedSemanticsResolution::DuplicateLabel {} => {
            UnsupportedSemanticsTag::DuplicateLabel
        }
        UnsupportedSemanticsResolution::ExternalInventory {} => {
            UnsupportedSemanticsTag::ExternalInventory
        }
        UnsupportedSemanticsResolution::Fragment { .. } => UnsupportedSemanticsTag::Fragment,
        UnsupportedSemanticsResolution::NetworkPath {} => UnsupportedSemanticsTag::NetworkPath,
        UnsupportedSemanticsResolution::Query { .. } => UnsupportedSemanticsTag::Query,
        UnsupportedSemanticsResolution::SiteRoute {} => UnsupportedSemanticsTag::SiteRoute,
    }
}
