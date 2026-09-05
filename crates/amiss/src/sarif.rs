mod model;
mod tests;

use std::collections::BTreeSet;

use amiss_wire::report::model::{Finding, FindingFix, FindingLocation, ReportPayload};
use amiss_wire::report::{Disposition, FindingKind};

use model::{
    ArtifactChange, ArtifactLocation, ByteRegion, Descriptor, Driver, FindingResult, Fingerprints,
    Fix, Invocation, Level, Location, Log, Message, Notification, PhysicalLocation, Region,
    Replacement, Rule, Run, Tool,
};

/// The SARIF projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. Findings become results
/// under their kind's rule, retained analysis errors become tool execution
/// notifications, and the finding key rides as the stable fingerprint.
pub(crate) fn log<P, R, M, E>(
    payload: &ReportPayload<P, R, M, E>,
    path_text: impl Fn(&P) -> Option<&str> + Copy,
) -> Log<'_> {
    let present_names: BTreeSet<FindingKind> =
        payload.findings.iter().map(|row| row.kind).collect();
    let present: Vec<FindingKind> = FindingKind::all()
        .filter(|kind| present_names.contains(kind))
        .collect();

    Log {
        schema: "https://json.schemastore.org/sarif-2.1.0.json",
        runs: [Run {
            invocations: [Invocation {
                execution_successful: payload.result.complete,
                exit_code: payload.result.exit_code,
                tool_execution_notifications: payload
                    .errors
                    .iter()
                    .map(|row| Notification {
                        descriptor: Descriptor { id: row.code },
                        level: Level::Error,
                        message: Message {
                            text: &row.description,
                        },
                    })
                    .collect(),
            }],
            results: payload
                .findings
                .iter()
                .map(|row| finding_result(row, &present, path_text))
                .collect(),
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

fn finding_result<'report, P, E>(
    row: &'report Finding<P, E>,
    present: &[FindingKind],
    path_text: impl Fn(&P) -> Option<&str> + Copy,
) -> FindingResult<'report> {
    FindingResult {
        fixes: row.fix.as_ref().map(|value| [fix(value)]),
        level: match row.effective_disposition {
            Disposition::Fail => Level::Error,
            Disposition::Warn => Level::Warning,
            Disposition::Record => Level::Note,
        },
        locations: location(&row.location, path_text).map(|location| [location]),
        message: Message {
            text: &row.description,
        },
        partial_fingerprints: Fingerprints {
            finding_key: row.finding_key,
        },
        rule_id: row.kind,
        rule_index: present.iter().position(|candidate| *candidate == row.kind),
    }
}

/// A wire fix renders as one SARIF fix: the byte region to delete and the
/// replacement text, under the engine's own fix description.
fn fix(fix: &FindingFix) -> Fix<'_> {
    Fix {
        artifact_changes: [ArtifactChange {
            artifact_location: ArtifactLocation {
                uri: uri(fix.path.as_str()),
            },
            replacements: [Replacement {
                deleted_region: ByteRegion {
                    byte_length: fix.span.end_byte.saturating_sub(fix.span.start_byte),
                    byte_offset: fix.span.start_byte,
                },
                inserted_content: Message {
                    text: &fix.replacement,
                },
            }],
        }],
        description: Message {
            text: &fix.description,
        },
    }
}

/// A location renders only when the wire path is printable text; a
/// `bytes_hex` path names no artifact URI, and the row still carries it.
fn location<P>(
    location: &FindingLocation<P>,
    path_text: impl Fn(&P) -> Option<&str>,
) -> Option<Location> {
    let path = location.path.as_ref().and_then(path_text)?;
    Some(Location {
        physical_location: PhysicalLocation {
            artifact_location: ArtifactLocation { uri: uri(path) },
            region: location.span.map(|span| Region {
                end_column: span.end_column,
                end_line: span.end_line,
                start_column: span.start_column,
                start_line: span.start_line,
            }),
        },
    })
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
