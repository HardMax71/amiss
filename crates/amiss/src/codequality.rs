use amiss_wire::json::Value;

use crate::view::{View, object, string};

/// The Code Quality projection: GitLab's merge-request artifact over the same
/// payload, one issue per finding row in report order, the finding key riding
/// as the fingerprint GitLab diffs between target and head. The format has no
/// shape for analysis errors or refusals, so those stay on the exit class and
/// the other lanes, and like every projection it cannot change facts,
/// ordering, totals, or exit.
pub(crate) fn issues(envelope: &Value) -> Value {
    let findings = View::of(Some(envelope)).view("payload").rows("findings");
    Value::Array(findings.iter().map(issue).collect())
}

fn issue(row: &View) -> Value {
    let severity = match row.text("effective_disposition").as_str() {
        "fail" => "major",
        "warn" => "minor",
        _ => "info",
    };
    object(vec![
        ("check_name", string(&row.text("kind"))),
        ("description", string(&row.text("description"))),
        ("fingerprint", string(&row.text("finding_key"))),
        ("location", location_value(&row.view("location"))),
        ("severity", string(severity)),
    ])
}

/// GitLab requires a path and a first line on every issue, so a byte-named
/// document answers with the wire's own hex spelling, a finding on no file
/// (the wire's nullable global side) answers as `(global)`, and a byte-only
/// span reads as line one.
fn location_value(location: &View) -> Value {
    let path = match location.field("path") {
        Some(Value::String(path)) => path.clone(),
        Some(Value::Object(_)) => location.view("path").text("bytes_hex"),
        _ => "(global)".to_owned(),
    };
    let begin = location.view("span").number("start_line").max(1);
    object(vec![
        ("lines", object(vec![("begin", Value::Integer(begin))])),
        ("path", string(&path)),
    ])
}

#[path = "../tests/internal/codequality.rs"]
mod tests;
