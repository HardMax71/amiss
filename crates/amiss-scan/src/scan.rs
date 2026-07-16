use amiss_md::extract::GovernedDefinition;
use amiss_md::lines::{Line, scan};
use amiss_md::{Occurrence, Opaque, Work, analyze};
use amiss_wire::digest::{Digest, hb};
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
/// and the digest of its exact contributing source bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedSource {
    pub span: (usize, usize),
    pub display: SpanDisplay,
    pub digest: Digest,
}

pub const GOVERNED_SOURCE_DOMAIN: &str = "amiss/scanner-governed-definition-source";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub adapter: Adapter,
    pub work: Work,
    pub occurrences: Vec<ScannedOccurrence>,
    pub opaque: Opaque,
    pub governed: Vec<GovernedSource>,
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
    let analysis = analyze(adapter, source).map_err(Error::Parse)?;
    resources.charge_work(analysis.work.nodes, analysis.work.nesting)?;

    let Some(extraction) = analysis.extraction else {
        return Ok(Scanned {
            adapter,
            work: analysis.work,
            occurrences: Vec::new(),
            opaque: Opaque::default(),
            governed: Vec::new(),
        });
    };

    let lines: Vec<Line> = scan(source).collect();
    let mut occurrences = Vec::with_capacity(extraction.occurrences.len());
    let mut document_references: u64 = 0;
    for occurrence in extraction.occurrences {
        document_references = document_references.saturating_add(1);
        resources.charge_reference(
            length(occurrence.raw_destination.as_bytes()),
            document_references,
        )?;
        let block = source
            .get(occurrence.block_span.0..occurrence.block_span.1)
            .ok_or(Error::Parse(amiss_md::Fault::InvalidSourceSpan))?;
        let display = SpanDisplay {
            start_line: position(source, &lines, occurrence.span.0).0,
            start_column: position(source, &lines, occurrence.span.0).1,
            end_line: position(source, &lines, occurrence.span.1).0,
            end_column: position(source, &lines, occurrence.span.1).1,
        };
        occurrences.push(ScannedOccurrence {
            projection_digest: hb(SOURCE_PROJECTION_DOMAIN, &normalize_newlines(block)),
            raw_destination_digest: hb(
                RAW_DESTINATION_DOMAIN,
                occurrence.raw_destination.as_bytes(),
            ),
            display,
            occurrence,
        });
    }

    let mut governed = Vec::with_capacity(extraction.governed.len());
    for GovernedDefinition { span } in &extraction.governed {
        document_references = document_references.saturating_add(1);
        resources.charge_reference(0, document_references)?;
        let bytes = source
            .get(span.0..span.1)
            .ok_or(Error::Parse(amiss_md::Fault::InvalidSourceSpan))?;
        governed.push(GovernedSource {
            span: *span,
            display: SpanDisplay {
                start_line: position(source, &lines, span.0).0,
                start_column: position(source, &lines, span.0).1,
                end_line: position(source, &lines, span.1).0,
                end_column: position(source, &lines, span.1).1,
            },
            digest: hb(GOVERNED_SOURCE_DOMAIN, bytes),
        });
    }

    Ok(Scanned {
        adapter,
        work: analysis.work,
        occurrences,
        opaque: extraction.opaque,
        governed,
    })
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
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
fn position(source: &[u8], lines: &[Line], at: usize) -> (u64, u64) {
    let index = lines.partition_point(|line| line.end <= at);
    let start = lines.get(index).map_or_else(
        || lines.last().map_or(0, |line| line.end),
        |line| line.start,
    );
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
