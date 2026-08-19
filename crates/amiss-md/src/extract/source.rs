use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::Fault;
use markdown::mdast::ReferenceKind;

use super::span::span_of;
use super::{destination_token, skip_code_span, skip_whitespace};

pub(super) const fn reference_link(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::Full => SourceConstruct::FullReferenceLink,
        ReferenceKind::Collapsed => SourceConstruct::CollapsedReferenceLink,
        ReferenceKind::Shortcut => SourceConstruct::ShortcutReferenceLink,
    }
}

pub(super) const fn reference_image(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::Full => SourceConstruct::FullReferenceImage,
        ReferenceKind::Collapsed => SourceConstruct::CollapsedReferenceImage,
        ReferenceKind::Shortcut => SourceConstruct::ShortcutReferenceImage,
    }
}

/// Classifies a parsed link by its first source byte: `[` opens an inline
/// link, `<` an angle autolink, and anything else is a GFM extended autolink
/// whose final match is the node's own span. All autolink forms share one
/// construct; span and token distinguish them.
pub(super) fn link_destination(
    bytes: &[u8],
    suffix: &str,
    span: (usize, usize),
    link: &markdown::mdast::Link,
) -> Result<(SourceConstruct, String), Fault> {
    let first = bytes.get(span.0).copied().ok_or(Fault::InvalidSourceSpan)?;
    match first {
        b'[' => {
            let children_end = link
                .children
                .last()
                .map_or(Ok(span.0.saturating_add(1)), |child| {
                    span_of(child).map(|child_span| child_span.1)
                })?;
            let token_span = inline_destination(bytes, children_end)?;
            Ok((SourceConstruct::InlineLink, token(suffix, token_span)?))
        }
        b'<' => {
            if span.1 <= span.0.saturating_add(2) {
                return Err(Fault::InvalidSourceSpan);
            }
            let inside = (span.0.saturating_add(1), span.1.saturating_sub(1));
            Ok((SourceConstruct::Autolink, token(suffix, inside)?))
        }
        _ => Ok((SourceConstruct::Autolink, token(suffix, span)?)),
    }
}

/// Walks past `](`, any separating whitespace, and returns the destination
/// token: the inside of an angle form without its delimiters, or the bare run
/// under `CommonMark` escape and balanced-parenthesis rules.
pub(super) fn inline_destination(
    bytes: &[u8],
    children_end: usize,
) -> Result<(usize, usize), Fault> {
    if bytes.get(children_end) != Some(&b']') {
        return Err(Fault::InvalidSourceSpan);
    }
    let after = children_end.saturating_add(1);
    if bytes.get(after) != Some(&b'(') {
        return Err(Fault::InvalidSourceSpan);
    }
    let at = skip_whitespace(bytes, after.saturating_add(1));
    destination_token(bytes, at)
}

/// An image's label is flattened to a string in the tree, so its end is
/// recovered by scanning the source: brackets nest (an image label, unlike a
/// link label, may contain links), backslash escapes hide a bracket, and a
/// code span protects everything inside it.
pub(super) fn image_label_end(bytes: &[u8], span: (usize, usize)) -> Result<usize, Fault> {
    let mut at = span.0.saturating_add(2);
    let mut depth = 1_usize;
    while at < span.1 {
        let Some(&byte) = bytes.get(at) else {
            break;
        };
        match byte {
            b'\\' => at = at.saturating_add(2),
            b'`' => at = skip_code_span(bytes, at, span.1),
            b'[' => {
                depth = depth.saturating_add(1);
                at = at.saturating_add(1);
            }
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(at);
                }
                at = at.saturating_add(1);
            }
            _ => at = at.saturating_add(1),
        }
    }
    Err(Fault::ParserError)
}

/// The raw destination token and whether it was written in angle brackets.
pub(super) fn definition_destination(
    suffix: &str,
    span: (usize, usize),
) -> Result<(String, bool), Fault> {
    let bytes = suffix.as_bytes();
    let mut label = span.0;
    while matches!(bytes.get(label), Some(&(b' ' | b'\t'))) {
        label = label.saturating_add(1);
    }
    if bytes.get(label) != Some(&b'[') {
        return Err(Fault::InvalidSourceSpan);
    }
    let mut at = label.saturating_add(1);
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' => at = at.saturating_add(2),
            b']' => break,
            _ => at = at.saturating_add(1),
        }
    }
    if bytes.get(at) != Some(&b']') {
        return Err(Fault::InvalidSourceSpan);
    }
    if bytes.get(at.saturating_add(1)) != Some(&b':') {
        return Err(Fault::InvalidSourceSpan);
    }
    let start = skip_whitespace(bytes, at.saturating_add(2));
    let angled = bytes.get(start) == Some(&b'<');
    let token_span = destination_token(bytes, start)?;
    Ok((token(suffix, token_span)?, angled))
}

pub(super) fn token(suffix: &str, span: (usize, usize)) -> Result<String, Fault> {
    suffix
        .get(span.0..span.1)
        .map(str::to_owned)
        .ok_or(Fault::InvalidSourceSpan)
}
