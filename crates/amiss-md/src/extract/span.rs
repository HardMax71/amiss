use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::{Fault, Heading, Occurrence, Opaque};
use markdown::mdast::Node;

pub(super) fn span_of(node: &Node) -> Result<(usize, usize), Fault> {
    let position = node.position().ok_or(Fault::InvalidSourceSpan)?;
    let span = (position.start.offset, position.end.offset);
    if span.0 > span.1 {
        return Err(Fault::InvalidSourceSpan);
    }
    Ok(span)
}

/// Sorts by `(start, end)`, discards any span contained in another, and unions
/// overlapping or exactly adjacent spans into maximal disjoint intervals.
pub(super) fn union(mut spans: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if let Some(last) = out.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        out.push((start, end));
    }
    out
}

/// The closed source contract on every published span: inside the document,
/// not reversed, never splitting a CRLF pair, the opaque partition disjoint,
/// and every retained opaque region nonempty.
pub(super) fn validate(
    occurrences: &[Occurrence],
    headings: &[Heading],
    opaque: &Opaque,
    offset: usize,
    suffix_len: usize,
    raw: &[u8],
) -> Result<(), Fault> {
    let endpoint = |at: usize| -> bool {
        let translated = at.saturating_add(offset);
        !(translated > 0
            && raw.get(translated.wrapping_sub(1)) == Some(&b'\r')
            && raw.get(translated) == Some(&b'\n'))
    };
    let bounded = |span: (usize, usize)| -> bool {
        span.0 <= span.1 && span.1 <= suffix_len && endpoint(span.0) && endpoint(span.1)
    };
    for entry in occurrences {
        if !bounded(entry.span) || !bounded(entry.block_span) || entry.span.0 == entry.span.1 {
            return Err(Fault::InvalidSourceSpan);
        }
    }
    for heading in headings {
        if !bounded(heading.span) || heading.span.0 == heading.span.1 {
            return Err(Fault::InvalidSourceSpan);
        }
    }
    let mut regions: Vec<(usize, usize)> = Vec::new();
    regions.extend(opaque.mdx.iter().copied());
    regions.extend(opaque.html.iter().copied());
    regions.sort_unstable();
    let mut previous_end = 0_usize;
    for (index, region) in regions.iter().enumerate() {
        if !bounded(*region) || region.0 == region.1 {
            return Err(Fault::InvalidSourceSpan);
        }
        if index > 0 && region.0 < previous_end {
            return Err(Fault::InvalidSourceSpan);
        }
        previous_end = region.1;
    }
    Ok(())
}

type SpanCore = fn(&[u8], (usize, usize), &str) -> Option<(usize, usize)>;

/// One construct gate in front of both wire span cores: an autolink is a URL
/// or email address, so its hash can sit in a local part and its text never
/// names a repository path.
pub(super) fn gated_span(
    core: SpanCore,
    source: &[u8],
    span: (usize, usize),
    raw_destination: &str,
    construct: SourceConstruct,
) -> Option<(usize, usize)> {
    if construct == SourceConstruct::Autolink {
        return None;
    }
    core(source, span, raw_destination)
}
