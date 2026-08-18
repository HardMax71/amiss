use std::collections::BTreeSet;

use amiss_wire::json::Value;
use amiss_wire::report::FindingKind;

use crate::view::{View, object, string};

/// The SARIF projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. Findings become results
/// under their kind's rule, retained analysis errors become tool execution
/// notifications, and the finding key rides as the stable fingerprint.
pub(crate) fn log(envelope: &Value) -> Value {
    let payload = View::of(envelope).view("payload");
    let result = payload.view("result");
    let findings = payload.rows("findings");
    let errors = payload.rows("errors");

    let present_names: BTreeSet<&str> = findings.clone().map(|row| row.text("kind")).collect();
    let present: Vec<FindingKind> = FindingKind::all()
        .filter(|kind| present_names.contains(kind.as_str()))
        .collect();
    let rules: Vec<Value> = present
        .iter()
        .map(|kind| {
            object(vec![
                ("id", string(kind.as_str())),
                (
                    "shortDescription",
                    object(vec![("text", string(kind.meaning()))]),
                ),
            ])
        })
        .collect();

    let results: Vec<Value> = findings.map(|row| result_value(row, &present)).collect();
    let notifications: Vec<Value> = errors
        .map(|row| {
            object(vec![
                ("descriptor", object(vec![("id", string(row.text("code")))])),
                ("level", string("error")),
                (
                    "message",
                    object(vec![("text", string(row.text("description")))]),
                ),
            ])
        })
        .collect();

    let invocation = object(vec![
        ("executionSuccessful", Value::Bool(result.flag("complete"))),
        ("exitCode", Value::Integer(result.number("exit_code"))),
        ("toolExecutionNotifications", Value::Array(notifications)),
    ]);
    let driver = object(vec![
        (
            "informationUri",
            string("https://hardmax71.github.io/amiss/"),
        ),
        ("name", string("amiss")),
        ("rules", Value::Array(rules)),
        ("semanticVersion", string(env!("CARGO_PKG_VERSION"))),
    ]);
    let run = object(vec![
        ("invocations", Value::Array(vec![invocation])),
        ("results", Value::Array(results)),
        ("tool", object(vec![("driver", driver)])),
    ]);
    object(vec![
        (
            "$schema",
            string("https://json.schemastore.org/sarif-2.1.0.json"),
        ),
        ("runs", Value::Array(vec![run])),
        ("version", string("2.1.0")),
    ])
}

fn result_value(row: View<'_>, present: &[FindingKind]) -> Value {
    let kind = row.text("kind");
    let level = match row.text("effective_disposition") {
        "fail" => "error",
        "warn" => "warning",
        _ => "note",
    };
    let mut members = vec![
        ("level", string(level)),
        (
            "message",
            object(vec![("text", string(row.text("description")))]),
        ),
        (
            "partialFingerprints",
            object(vec![(
                "amissFindingKey/v1",
                string(row.text("finding_key")),
            )]),
        ),
        ("ruleId", string(kind)),
    ];
    if let Some(index) = present
        .iter()
        .position(|candidate| candidate.as_str() == kind)
        .and_then(|position| i64::try_from(position).ok())
    {
        members.push(("ruleIndex", Value::Integer(index)));
    }
    if let Some(location) = location_value(row.view("location")) {
        members.push(("locations", Value::Array(vec![location])));
    }
    if let Some(fix) = fix_value(row.view("fix")) {
        members.push(("fixes", Value::Array(vec![fix])));
    }
    object(members)
}

/// A wire fix renders as one SARIF fix: the byte region to delete and the
/// replacement text, under the engine's own fix description.
fn fix_value(fix: View<'_>) -> Option<Value> {
    let location = artifact_location(fix)?;
    let Some(Value::String(replacement)) = fix.field("replacement") else {
        return None;
    };
    let span = fix.view("span");
    let start = span.number("start_byte");
    let length = span.number("end_byte").saturating_sub(start);
    Some(object(vec![
        (
            "artifactChanges",
            Value::Array(vec![object(vec![
                ("artifactLocation", location),
                (
                    "replacements",
                    Value::Array(vec![object(vec![
                        (
                            "deletedRegion",
                            object(vec![
                                ("byteLength", Value::Integer(length)),
                                ("byteOffset", Value::Integer(start)),
                            ]),
                        ),
                        (
                            "insertedContent",
                            object(vec![("text", string(replacement))]),
                        ),
                    ])]),
                ),
            ])]),
        ),
        (
            "description",
            object(vec![("text", string(fix.text("description")))]),
        ),
    ]))
}

/// A location renders only when the wire path is printable text; a
/// `bytes_hex` path names no artifact URI, and the row still carries it.
fn location_value(location: View<'_>) -> Option<Value> {
    let mut physical = vec![("artifactLocation", artifact_location(location)?)];
    let span = location.view("span");
    if span.field("start_line").is_some() {
        physical.push((
            "region",
            object(vec![
                ("endColumn", Value::Integer(span.number("end_column"))),
                ("endLine", Value::Integer(span.number("end_line"))),
                ("startColumn", Value::Integer(span.number("start_column"))),
                ("startLine", Value::Integer(span.number("start_line"))),
            ]),
        ));
    }
    Some(object(vec![("physicalLocation", object(physical))]))
}

/// The artifact holding a location or a fix, named only when the wire path
/// is printable text.
fn artifact_location(holder: View<'_>) -> Option<Value> {
    let Some(Value::String(path)) = holder.field("path") else {
        return None;
    };
    Some(object(vec![("uri", string(&uri(path)))]))
}

/// RFC 3986 path form: unreserved bytes and the separator stay, every other
/// byte is percent-encoded so a hostile path cannot break the URI.
fn uri(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            let high = char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0');
            let low = char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0');
            encoded.push(high.to_ascii_uppercase());
            encoded.push(low.to_ascii_uppercase());
        }
    }
    encoded
}
