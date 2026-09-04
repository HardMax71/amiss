mod model;
mod tests;

use std::collections::BTreeSet;

use amiss_wire::json::Value;
use amiss_wire::report::FindingKind;

use crate::view::View;
use model::{
    ArtifactChange, ArtifactLocation, ByteRegion, Descriptor, Driver, FindingResult, Fingerprints,
    Fix, Invocation, Level, Location, Log, Message, Notification, PhysicalLocation, Region,
    Replacement, Rule, Run, Tool,
};

/// The SARIF projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. Findings become results
/// under their kind's rule, retained analysis errors become tool execution
/// notifications, and the finding key rides as the stable fingerprint.
pub(crate) fn log(envelope: &Value) -> Log<'_> {
    let payload = View::of(envelope).view("payload");
    let result = payload.view("result");
    let findings = payload.rows("findings");
    let present_names: BTreeSet<&str> = findings.clone().map(|row| row.text("kind")).collect();
    let present: Vec<FindingKind> = FindingKind::all()
        .filter(|kind| present_names.contains(kind.as_ref()))
        .collect();

    Log {
        schema: "https://json.schemastore.org/sarif-2.1.0.json",
        runs: [Run {
            invocations: [Invocation {
                execution_successful: result.flag("complete"),
                exit_code: result.number("exit_code"),
                tool_execution_notifications: payload
                    .rows("errors")
                    .map(|row| Notification {
                        descriptor: Descriptor {
                            id: row.text("code"),
                        },
                        level: Level::Error,
                        message: Message {
                            text: row.text("description"),
                        },
                    })
                    .collect(),
            }],
            results: findings.map(|row| finding_result(row, &present)).collect(),
            tool: Tool {
                driver: Driver {
                    information_uri: "https://hardmax71.github.io/amiss/",
                    name: "amiss",
                    rules: present
                        .into_iter()
                        .map(|kind| Rule {
                            id: kind,
                            short_description: Message {
                                text: kind.meaning(),
                            },
                        })
                        .collect(),
                    semantic_version: env!("CARGO_PKG_VERSION"),
                },
            },
        }],
        version: "2.1.0",
    }
}

fn finding_result<'report>(row: View<'report>, present: &[FindingKind]) -> FindingResult<'report> {
    let kind = row.text("kind");
    FindingResult {
        fixes: fix(row.view("fix")).map(|fix| [fix]),
        level: match row.text("effective_disposition") {
            "fail" => Level::Error,
            "warn" => Level::Warning,
            _ => Level::Note,
        },
        locations: location(row.view("location")).map(|location| [location]),
        message: Message {
            text: row.text("description"),
        },
        partial_fingerprints: Fingerprints {
            finding_key: row.text("finding_key"),
        },
        rule_id: kind,
        rule_index: present
            .iter()
            .position(|candidate| candidate.as_ref() == kind),
    }
}

/// A wire fix renders as one SARIF fix: the byte region to delete and the
/// replacement text, under the engine's own fix description.
fn fix(fix: View<'_>) -> Option<Fix<'_>> {
    let artifact_location = artifact_location(fix)?;
    let Some(Value::String(replacement)) = fix.field("replacement") else {
        return None;
    };
    let span = fix.view("span");
    let start = span.number("start_byte");
    Some(Fix {
        artifact_changes: [ArtifactChange {
            artifact_location,
            replacements: [Replacement {
                deleted_region: ByteRegion {
                    byte_length: span.number("end_byte").saturating_sub(start),
                    byte_offset: start,
                },
                inserted_content: Message { text: replacement },
            }],
        }],
        description: Message {
            text: fix.text("description"),
        },
    })
}

/// A location renders only when the wire path is printable text; a
/// `bytes_hex` path names no artifact URI, and the row still carries it.
fn location(location: View<'_>) -> Option<Location> {
    let artifact_location = artifact_location(location)?;
    let span = location.view("span");
    Some(Location {
        physical_location: PhysicalLocation {
            artifact_location,
            region: span.field("start_line").map(|_line| Region {
                end_column: span.number("end_column"),
                end_line: span.number("end_line"),
                start_column: span.number("start_column"),
                start_line: span.number("start_line"),
            }),
        },
    })
}

/// The artifact holding a location or a fix, named only when the wire path
/// is printable text.
fn artifact_location(holder: View<'_>) -> Option<ArtifactLocation> {
    let Some(Value::String(path)) = holder.field("path") else {
        return None;
    };
    Some(ArtifactLocation { uri: uri(path) })
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
