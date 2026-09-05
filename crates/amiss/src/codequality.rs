mod tests;

use amiss_wire::digest::Digest;
use amiss_wire::report::model::ReportPayload;
use amiss_wire::report::{Disposition, FindingKind};
use serde::Serialize;
use std::borrow::Cow;

#[derive(Serialize)]
pub(crate) struct Issue<'report> {
    check_name: FindingKind,
    description: &'report str,
    fingerprint: Digest,
    location: Location<'report>,
    severity: Severity,
}

#[derive(Serialize)]
struct Location<'report> {
    lines: Lines,
    path: Cow<'report, str>,
}

#[derive(Serialize)]
struct Lines {
    begin: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Info,
    Major,
    Minor,
}

/// The Code Quality projection: GitLab's merge-request artifact over the same
/// payload, one issue per finding row in report order, the finding key riding
/// as the fingerprint GitLab diffs between target and head. The format has no
/// shape for analysis errors or refusals, so those stay on the exit class and
/// the other lanes, and like every projection it cannot change facts,
/// ordering, totals, or exit.
pub(crate) fn issues<'report, P, R, M, E>(
    payload: &'report ReportPayload<P, R, M, E>,
    path_label: impl Fn(&'report P) -> Cow<'report, str>,
) -> Vec<Issue<'report>> {
    payload
        .findings
        .iter()
        .map(|row| {
            let location = &row.location;
            Issue {
                check_name: row.kind,
                description: &row.description,
                fingerprint: row.finding_key,
                location: Location {
                    lines: Lines {
                        begin: location.span.map_or(1, |span| span.start_line.max(1)),
                    },
                    path: location
                        .path
                        .as_ref()
                        .map_or(Cow::Borrowed("(global)"), &path_label),
                },
                severity: match row.effective_disposition {
                    Disposition::Fail => Severity::Major,
                    Disposition::Warn => Severity::Minor,
                    Disposition::Record => Severity::Info,
                },
            }
        })
        .collect()
}
