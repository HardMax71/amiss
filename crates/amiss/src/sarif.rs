mod model;
mod tests;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::model::Digest;
use amiss_wire::report::model::{FindingFix, FindingLocation, ReportPayload};
use amiss_wire::report::{Disposition, FindingKind};

use model::{
    ArtifactChange, ArtifactLocation, ByteRegion, Descriptor, Driver, FindingResult, Fingerprints,
    Fix, Invocation, Level, Location, Log, Message, Notification, PhysicalLocation, Region,
    Replacement, Rule, Run, Tool,
};

/// The SARIF projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. Findings become results
/// under their kind's rule, retained analysis errors become tool execution
/// notifications, and the finding key rides as the stable fingerprint. A
/// row's message leads with its own words where it has some, so two broken
/// links under one rule do not read alike.
pub(crate) fn log<'report, P, R, M, E>(
    payload: &'report ReportPayload<P, R, M, E>,
    path_bytes: impl Fn(&P) -> Option<Cow<'_, [u8]>> + Copy,
    words: &BTreeMap<Digest, String>,
) -> Log<'report> {
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
                            text: Cow::Borrowed(&row.description),
                        },
                    })
                    .collect(),
            }],
            results: payload
                .findings
                .iter()
                .map(|row| FindingResult {
                    fixes: row.fix.as_ref().map(|value| [fix(value)]),
                    level: match row.effective_disposition {
                        Disposition::Fail => Level::Error,
                        Disposition::Warn => Level::Warning,
                        Disposition::Record => Level::Note,
                    },
                    locations: location(&row.location, path_bytes).map(|location| [location]),
                    message: Message {
                        text: words
                            .get(&row.finding_key)
                            .map_or(Cow::Borrowed(row.description.as_str()), |words| {
                                Cow::Owned(format!("{words}: {}", row.description))
                            }),
                    },
                    partial_fingerprints: Fingerprints {
                        finding_key: row.finding_key,
                    },
                    rule_id: row.kind,
                    rule_index: present.iter().position(|candidate| *candidate == row.kind),
                })
                .collect(),
            tool: Tool {
                driver: Driver {
                    information_uri: "https://hardmax71.github.io/amiss/",
                    name: "amiss",
                    rules: present
                        .into_iter()
                        .map(|kind| Rule {
                            help: Message {
                                text: Cow::Borrowed(kind.meaning()),
                            },
                            help_uri: "https://hardmax71.github.io/amiss/profiles.html",
                            id: kind,
                            short_description: Message {
                                text: Cow::Borrowed(kind.meaning()),
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

/// A wire fix renders as one SARIF fix: the byte region to delete and the
/// replacement text, under the engine's own fix description.
fn fix(fix: &FindingFix) -> Fix<'_> {
    Fix {
        artifact_changes: [ArtifactChange {
            artifact_location: ArtifactLocation {
                uri: percent_encoding::utf8_percent_encode(fix.path.as_str(), URI_PATH_ENCODE_SET)
                    .to_string(),
            },
            replacements: [Replacement {
                deleted_region: ByteRegion {
                    byte_length: fix.span.end_byte.saturating_sub(fix.span.start_byte),
                    byte_offset: fix.span.start_byte,
                },
                inserted_content: Message {
                    text: Cow::Borrowed(&fix.replacement),
                },
            }],
        }],
        description: Message {
            text: Cow::Borrowed(&fix.description),
        },
    }
}

/// A location's URI percent-encodes the path's own bytes, so a name that is
/// not UTF-8 still names its file, the same bytes a checkout writes.
fn location<P>(
    location: &FindingLocation<P>,
    path_bytes: impl Fn(&P) -> Option<Cow<'_, [u8]>>,
) -> Option<Location> {
    let path = location.path.as_ref().and_then(path_bytes)?;
    Some(Location {
        physical_location: PhysicalLocation {
            artifact_location: ArtifactLocation {
                uri: percent_encoding::percent_encode(&path, URI_PATH_ENCODE_SET).to_string(),
            },
            region: location.span.map(|span| Region {
                end_column: span.end_column,
                end_line: span.end_line,
                start_column: span.start_column,
                start_line: span.start_line,
            }),
        },
    })
}

const URI_PATH_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');
