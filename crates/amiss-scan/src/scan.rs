use std::borrow::Cow;

use amiss_md::lines::scan;
use amiss_md::{Analysis, AnalyzeError, Occurrence, Opaque, Work, analyze};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::extraction::GovernedDefinition;
use amiss_wire::model::Adapter;

use crate::resources::ScanResources;
use crate::{Error, RAW_DESTINATION_DOMAIN, SOURCE_PROJECTION_DOMAIN};

/// One-based Unicode-scalar display positions for a machine byte span, after
/// the same CRLF and bare-CR to LF conversion the projection applies. A tab is
/// one scalar and no display-width expansion occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanDisplay {
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
}

/// One extracted occurrence enriched with what the report needs beyond the
/// corpus goldens: display positions, the containing block's projection
/// digest, and the raw destination digest, where an empty destination hashes
/// zero bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannedOccurrence {
    pub occurrence: Occurrence,
    pub display: SpanDisplay,
    pub projection_digest: Digest,
    pub raw_destination_digest: Digest,
}

/// One reserved governed definition with its raw span, display positions,
/// the digest of its exact contributing source bytes, and the claim form
/// its words spell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSource {
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub digest: Digest,
    pub form: crate::claim::GovernedForm,
    pub previous_code: Option<SemanticCodeSink>,
}

pub const GOVERNED_SOURCE_DOMAIN: &str = "amiss/scanner-governed-definition-source";
pub const PROJECTION_SINK_DOMAIN: &str = "amiss/scanner-projection-sink";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticCodeSink {
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub digest: Digest,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub adapter: Adapter,
    pub work: Work,
    pub embedded_code_bytes: u64,
    pub occurrences: Vec<ScannedOccurrence>,
    pub opaque: Opaque,
    pub governed: Vec<GovernedSource>,
    pub declared_anchors: Vec<String>,
    pub anchor_source: Option<AnchorSource>,
}

/// The raw anchor inputs a scanned document retains so the resolve lane never
/// parses an in-set target twice; slugging stays lazy, paid only for targets
/// a fragment actually asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorSource {
    pub headings: Vec<amiss_wire::extraction::Heading>,
    pub html_anchors: Vec<String>,
    pub transclusions: Vec<amiss_wire::extraction::Transclusion>,
}

/// Scans one selected document body under the snapshot's budgets: admission
/// first, then the guarded parse, then node work, then each reference in
/// document order. The first crossing or fault ends the document.
///
/// # Errors
///
/// `Parse` faults in the contract's precedence, and `ResourceLimit` crossings
/// under the closed observation laws.
pub fn scan_document(
    resources: &mut ScanResources,
    adapter: Adapter,
    source: &[u8],
) -> Result<Scanned, Error> {
    resources.charge_document(length(source))?;
    scan_bytes(resources, adapter, source)
}

/// The one place an adapter is chosen. Both the document scan and the
/// resolver's anchor read come through here.
///
/// # Errors
///
/// Whatever the chosen adapter refuses the bytes with.
pub(crate) fn parse(
    adapter: Adapter,
    source: &[u8],
    embedded_code_allowance: u64,
) -> Result<Analysis, AnalyzeError> {
    match adapter {
        Adapter::AsciiDoc => amiss_adoc::analyze(source),
        Adapter::Rst => amiss_rst::analyze(source),
        Adapter::Markdown | Adapter::Mdx | Adapter::PlainAdvisory => {
            analyze(adapter, source, embedded_code_allowance)
        }
    }
}

/// Parses and extracts one already admitted document body.
///
/// # Errors
///
/// Everything `scan_document` fails with except the admission crossings.
pub fn scan_bytes(
    resources: &mut ScanResources,
    adapter: Adapter,
    source: &[u8],
) -> Result<Scanned, Error> {
    let analysis = parse(adapter, source, resources.embedded_code_allowance()).map_err(
        |error| match error {
            AnalyzeError::Fault(fault) => Error::Parse(fault),
            AnalyzeError::EmbeddedCodeAllowance { spent } => {
                resources.embedded_code_crossing(spent)
            }
        },
    )?;
    resources.charge_embedded_code(analysis.embedded_code_bytes);
    resources.charge_work(analysis.work.nodes, analysis.work.nesting)?;

    let Some(extraction) = analysis.extraction else {
        return Ok(Scanned {
            adapter,
            work: analysis.work,
            embedded_code_bytes: analysis.embedded_code_bytes,
            occurrences: Vec::new(),
            opaque: Opaque::default(),
            governed: Vec::new(),
            declared_anchors: Vec::new(),
            anchor_source: None,
        });
    };

    let line_ends = if extraction.occurrences.is_empty() && extraction.governed.is_empty() {
        Vec::new()
    } else {
        scan(source).map(|line| line.end).collect()
    };
    let mut occurrences = Vec::with_capacity(extraction.occurrences.len());
    let mut document_references: u64 = 0;
    let mut previous_projection = None;
    for occurrence in extraction.occurrences {
        document_references = document_references.saturating_add(1);
        resources.charge_reference(
            length(occurrence.raw_destination.as_bytes()),
            document_references,
        )?;
        let projection_digest =
            source_projection_digest(source, occurrence.block_span, previous_projection)?;
        previous_projection = Some((occurrence.block_span, projection_digest));
        let (start_line, start_column) = position(source, &line_ends, occurrence.span.0);
        let (end_line, end_column) = position(source, &line_ends, occurrence.span.1);
        let display = SpanDisplay {
            start_line,
            start_column,
            end_line,
            end_column,
        };
        occurrences.push(ScannedOccurrence {
            projection_digest,
            raw_destination_digest: hb(
                RAW_DESTINATION_DOMAIN,
                occurrence.raw_destination.as_bytes(),
            ),
            display,
            occurrence,
        });
    }

    let governed = governed_sources(
        resources,
        source,
        &line_ends,
        &extraction.governed,
        document_references,
    )?;

    Ok(Scanned {
        adapter,
        work: analysis.work,
        embedded_code_bytes: analysis.embedded_code_bytes,
        occurrences,
        opaque: extraction.opaque,
        governed,
        declared_anchors: extraction.declared_anchors,
        anchor_source: Some(AnchorSource {
            headings: extraction.headings,
            html_anchors: extraction.html_anchors,
            transclusions: extraction.transclusions,
        }),
    })
}

fn governed_sources(
    resources: &mut ScanResources,
    source: &[u8],
    line_ends: &[usize],
    definitions: &[GovernedDefinition],
    mut document_references: u64,
) -> Result<Vec<GovernedSource>, Error> {
    let mut governed = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let span = definition.span;
        document_references = document_references.saturating_add(1);
        resources.charge_reference(0, document_references)?;
        let bytes = source
            .get(span.0..span.1)
            .ok_or(Error::Parse(amiss_md::Fault::InvalidSourceSpan))?;
        let (start_line, start_column) = position(source, line_ends, span.0);
        let (end_line, end_column) = position(source, line_ends, span.1);
        let previous_code = definition
            .previous_code
            .as_ref()
            .map(|code| {
                source
                    .get(code.span.0..code.span.1)
                    .ok_or(Error::Parse(amiss_md::Fault::InvalidSourceSpan))?;
                let (start_line, start_column) = position(source, line_ends, code.span.0);
                let (end_line, end_column) = position(source, line_ends, code.span.1);
                let value = crate::projection::normalized_line_endings(code.value.as_bytes());
                let value = std::str::from_utf8(value.as_ref())
                    .map_err(|_invalid| Error::Internal)?
                    .to_owned();
                Ok::<_, Error>(SemanticCodeSink {
                    span: code.span,
                    display: SpanDisplay {
                        start_line,
                        start_column,
                        end_line,
                        end_column,
                    },
                    digest: hb(PROJECTION_SINK_DOMAIN, value.as_bytes()),
                    value,
                })
            })
            .transpose()?;
        governed.push(GovernedSource {
            span,
            display: SpanDisplay {
                start_line,
                start_column,
                end_line,
                end_column,
            },
            digest: hb(GOVERNED_SOURCE_DOMAIN, bytes),
            form: crate::claim::classify(definition),
            previous_code,
        });
    }
    Ok(governed)
}

/// Replays a successful artifact against the independent snapshot ledger.
pub(crate) fn replay_scan_charges(
    resources: &mut ScanResources,
    scanned: &Scanned,
) -> Result<(), Error> {
    if scanned.embedded_code_bytes > resources.embedded_code_allowance() {
        return Err(resources.embedded_code_crossing(scanned.embedded_code_bytes));
    }
    resources.charge_embedded_code(scanned.embedded_code_bytes);
    resources.charge_work(scanned.work.nodes, scanned.work.nesting)?;

    let mut document_references = 0_u64;
    for occurrence in &scanned.occurrences {
        document_references = document_references.saturating_add(1);
        resources.charge_reference(
            length(occurrence.occurrence.raw_destination.as_bytes()),
            document_references,
        )?;
    }
    for _definition in &scanned.governed {
        document_references = document_references.saturating_add(1);
        resources.charge_reference(0, document_references)?;
    }
    Ok(())
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

fn source_projection_digest(
    source: &[u8],
    span: (usize, usize),
    previous: Option<((usize, usize), Digest)>,
) -> Result<Digest, Error> {
    previous
        .filter(|(cached, _)| *cached == span)
        .map(|(_, digest)| digest)
        .map_or_else(
            || {
                let block = source
                    .get(span.0..span.1)
                    .ok_or(Error::Parse(amiss_md::Fault::InvalidSourceSpan))?;
                let projected = if block.contains(&b'\r') {
                    Cow::Owned(normalize_newlines(block))
                } else {
                    Cow::Borrowed(block)
                };
                Ok(hb(SOURCE_PROJECTION_DOMAIN, projected.as_ref()))
            },
            Ok,
        )
}

/// `SourceProjection`: CRLF and bare CR become LF; every other source byte
/// is preserved, including final-newline presence.
#[must_use]
pub fn normalize_newlines(source: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(source.len());
    let mut at = 0_usize;
    while let Some(&byte) = source.get(at) {
        if byte == b'\r' {
            out.push(b'\n');
            if source.get(at.saturating_add(1)) == Some(&b'\n') {
                at = at.saturating_add(2);
                continue;
            }
        } else {
            out.push(byte);
        }
        at = at.saturating_add(1);
    }
    out
}

/// The line holding a byte offset is the first whose exclusive end is past
/// it; an offset past the final ending sits at column one of the next line.
/// Columns count Unicode scalars from the line start.
fn position(source: &[u8], line_ends: &[usize], at: usize) -> (u64, u64) {
    let index = line_ends.partition_point(|end| *end <= at);
    let start = index
        .checked_sub(1)
        .and_then(|previous| line_ends.get(previous))
        .copied()
        .unwrap_or(0);
    let line = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
    let scalars = source
        .get(start..at)
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .map_or(0, |text| text.chars().count());
    (
        line,
        u64::try_from(scalars).unwrap_or(u64::MAX).saturating_add(1),
    )
}
