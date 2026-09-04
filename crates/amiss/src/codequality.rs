mod tests;

use amiss_wire::json::Value;
use serde::Serialize;

use crate::view::View;

#[derive(Serialize)]
pub(crate) struct Issue<'report> {
    check_name: &'report str,
    description: &'report str,
    fingerprint: &'report str,
    location: Location<'report>,
    severity: Severity,
}

#[derive(Serialize)]
struct Location<'report> {
    lines: Lines,
    path: &'report str,
}

#[derive(Serialize)]
struct Lines {
    begin: i64,
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
pub(crate) fn issues(envelope: &Value) -> Vec<Issue<'_>> {
    View::of(envelope)
        .view("payload")
        .rows("findings")
        .map(|row| {
            let location = row.view("location");
            Issue {
                check_name: row.text("kind"),
                description: row.text("description"),
                fingerprint: row.text("finding_key"),
                location: Location {
                    lines: Lines {
                        begin: location.view("span").number("start_line").max(1),
                    },
                    path: match location.field("path") {
                        Some(Value::String(path)) => path,
                        Some(Value::Object(_)) => location.view("path").text("bytes_hex"),
                        _ => "(global)",
                    },
                },
                severity: match row.text("effective_disposition") {
                    "fail" => Severity::Major,
                    "warn" => Severity::Minor,
                    _ => Severity::Info,
                },
            }
        })
        .collect()
}
