use std::borrow::Cow;
use std::io::{BufWriter, Stdout};
use std::process::ExitCode;

use amiss_wire::ExitClass;
use amiss_wire::assessment::Nullable;
use amiss_wire::controls::MissingResolution;
use amiss_wire::envelope::Payload as _;
use amiss_wire::human::atom;
use amiss_wire::report::model::{ReportPayload, ReportResolution};
use amiss_wire::report::result_verdict;
use amiss_wire::resolution::{MissingTag, ResolutionTag, VersionScopeTag};

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
        |path| {
            path.as_str()
                .ok_or_else(|| Cow::Owned(hex::encode(path.as_bytes())))
        },
        |resolution| wire_resolution(resolution, crate::human::engine_path),
    );
    crate::projection_exit(result, ExitCode::from(verdict.code()))
}

/// A resolution read back from a report, spelled for a human place line over
/// the path form that report carries.
pub(crate) fn wire_resolution<P, F: Fn(&P) -> String>(
    resolution: &ReportResolution<P>,
    path: F,
) -> (ResolutionTag, Option<String>) {
    match resolution {
        ReportResolution::Missing(missing) => {
            let (tag, near, moved) = match missing {
                MissingResolution::PathNotFound {
                    near,
                    same_object_at,
                    ..
                } => (
                    MissingTag::PathNotFound,
                    near.as_ref().map(&path),
                    match same_object_at {
                        Some(Nullable::Value(moved)) => Some(path(moved)),
                        Some(Nullable::Null) | None => None,
                    },
                ),
                MissingResolution::HeadingAnchorNotFound { near, .. } => (
                    MissingTag::HeadingAnchorNotFound,
                    near.as_ref().map(|near| atom(near)),
                    None,
                ),
                MissingResolution::LineFragmentOutOfRange { .. } => {
                    (MissingTag::LineFragmentOutOfRange, None, None)
                }
                MissingResolution::LabelNotDeclared {} => {
                    (MissingTag::LabelNotDeclared, None, None)
                }
                MissingResolution::SelectionNotFound { .. } => {
                    (MissingTag::SelectionNotFound, None, None)
                }
            };
            (
                ResolutionTag::Missing,
                Some(missing_detail(tag, near, moved)),
            )
        }
        ReportResolution::Invalid { reason } => {
            (ResolutionTag::Invalid, Some(reason.as_ref().to_owned()))
        }
        ReportResolution::UnsupportedTarget { reason, .. } => (
            ResolutionTag::UnsupportedTarget,
            Some(reason.as_ref().to_owned()),
        ),
        ReportResolution::UnsupportedSemantics(semantics) => (
            ResolutionTag::UnsupportedSemantics,
            Some(semantics.reason.to_string()),
        ),
        ReportResolution::UnsupportedVersion { scope } => (
            ResolutionTag::UnsupportedVersion,
            Some(VersionScopeTag::from(scope).as_ref().to_owned()),
        ),
        ReportResolution::Resolved { .. } => (ResolutionTag::Resolved, None),
        ReportResolution::TypeMismatch { .. } => (ResolutionTag::TypeMismatch, None),
        ReportResolution::DeclaredUntracked { .. } => (ResolutionTag::DeclaredUntracked, None),
        ReportResolution::External { .. } => (ResolutionTag::External, None),
    }
}
