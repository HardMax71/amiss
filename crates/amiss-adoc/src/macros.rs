use crate::Title;
use amiss_wire::extraction::{TransclusionKind, TransclusionRefusal};

/// The reference forms this adapter reads, every one of them core `AsciiDoc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    CrossReference,
    InternalCrossReference,
    Link,
    BlockImage,
    InlineImage,
    Include,
}

impl ReferenceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CrossReference => "asciidoc-xref-macro",
            Self::InternalCrossReference => "asciidoc-internal-xref",
            Self::Link => "asciidoc-link-macro",
            Self::BlockImage => "asciidoc-block-image",
            Self::InlineImage => "asciidoc-inline-image",
            Self::Include => "asciidoc-include",
        }
    }

    #[must_use]
    pub const fn is_image(self) -> bool {
        matches!(self, Self::BlockImage | Self::InlineImage)
    }
}

/// One recognised reference. `target` is the exact source text before the
/// attribute list, so an unsubstituted `{attribute}` survives into it and the
/// resolver can refuse the destination rather than guess at a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub target: String,
    pub span: (usize, usize),
    pub block: usize,
    pub block_span: (usize, usize),
    pub list_item: bool,
    pub transclusion: Option<Result<TransclusionKind, TransclusionRefusal>>,
}

impl Reference {
    /// Whether the target still carries an attribute reference, which no tree
    /// can answer because the value arrives at build time.
    #[must_use]
    pub fn attribute_substituted(&self) -> bool {
        self.target.contains('{') && self.target.contains('}')
    }
}

const MACROS: [(&str, ReferenceKind); 4] = [
    ("xref:", ReferenceKind::CrossReference),
    ("link:", ReferenceKind::Link),
    ("image::", ReferenceKind::BlockImage),
    ("image:", ReferenceKind::InlineImage),
];

/// Reads one line's references. `at` is the line's byte offset in the document,
/// so every span is absolute.
#[must_use]
pub fn references(line: &str, at: usize) -> Vec<Reference> {
    let mut found = Vec::new();
    if let Some(rest) = line.strip_prefix("include::")
        && let Some((target, options, end)) = target_of(rest)
    {
        let mut reference = build(
            ReferenceKind::Include,
            target,
            at,
            0,
            end.saturating_add("include::".len()),
        );
        reference.transclusion = Some(if target.contains('{') && target.contains('}') {
            Err(TransclusionRefusal::DynamicTarget)
        } else if options.is_empty() {
            Ok(TransclusionKind::Parsed)
        } else {
            Err(TransclusionRefusal::Options)
        });
        found.push(reference);
        return found;
    }
    let skips = verbatim_spans(line);
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < line.len() {
        if skips
            .iter()
            .any(|(start, end)| index >= *start && index < *end)
        {
            index = index.saturating_add(1);
            continue;
        }
        if bytes.get(index.wrapping_sub(1)) == Some(&b'\\') {
            index = index.saturating_add(1);
            continue;
        }
        let tail = line.get(index..).unwrap_or_default();
        if let Some(reference) = internal(tail, at, index) {
            index = reference.span.1.saturating_sub(at);
            found.push(reference);
            continue;
        }
        if let Some(reference) = macro_at(line, tail, at, index) {
            index = reference.span.1.saturating_sub(at);
            found.push(reference);
            continue;
        }
        index = index.saturating_add(1);
    }
    found
}

fn macro_at(line: &str, tail: &str, at: usize, index: usize) -> Option<Reference> {
    if !boundary(line, index) {
        return None;
    }
    let (name, kind) = MACROS
        .iter()
        .find(|(name, _)| tail.starts_with(name))
        .copied()?;
    let rest = tail.get(name.len()..)?;
    let (target, _options, end) = target_of(rest)?;
    Some(build(
        kind,
        target,
        at,
        index,
        index.saturating_add(name.len()).saturating_add(end),
    ))
}

fn internal(tail: &str, at: usize, index: usize) -> Option<Reference> {
    let rest = tail.strip_prefix("<<")?;
    let close = rest.find(">>")?;
    let inside = rest.get(..close)?;
    if inside.is_empty() || inside.contains('<') {
        return None;
    }
    let target = inside.split(',').next().unwrap_or_default().trim();
    if target.is_empty() {
        return None;
    }
    Some(build(
        ReferenceKind::InternalCrossReference,
        target,
        at,
        index,
        index
            .saturating_add(2)
            .saturating_add(close)
            .saturating_add(2),
    ))
}

fn build(kind: ReferenceKind, target: &str, at: usize, start: usize, end: usize) -> Reference {
    Reference {
        kind,
        target: target.to_owned(),
        span: (at.saturating_add(start), at.saturating_add(end)),
        block: 0,
        block_span: (0, 0),
        list_item: false,
        transclusion: None,
    }
}

/// A macro target runs to the opening bracket of its attribute list. Whitespace
/// before that bracket means this was prose that happened to start with the
/// macro name.
fn target_of(rest: &str) -> Option<(&str, &str, usize)> {
    let open = rest.find('[')?;
    let target = rest.get(..open)?;
    if target.is_empty() || target.chars().any(char::is_whitespace) {
        return None;
    }
    let close = rest.get(open..)?.find(']')?;
    let options = rest.get(open.saturating_add(1)..open.saturating_add(close))?;
    Some((
        target,
        options,
        open.saturating_add(close).saturating_add(1),
    ))
}

/// The byte intervals a macro name cannot start in: monospace spans and inline
/// passthrough, which is where a document quoting `AsciiDoc` syntax puts it.
fn verbatim_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for fence in ['`', '+'] {
        let mut open: Option<usize> = None;
        for (index, character) in line.char_indices() {
            if character != fence {
                continue;
            }
            match open {
                Some(start) => {
                    spans.push((start, index.saturating_add(1)));
                    open = None;
                }
                None => open = Some(index),
            }
        }
    }
    spans
}

/// A macro name only opens a macro at the start of a word. Without this,
/// prose ending in a word that happens to close with the name would open one.
fn boundary(line: &str, index: usize) -> bool {
    line.get(..index)
        .and_then(|before| before.chars().next_back())
        .is_none_or(|character| !character.is_alphanumeric() && character != '_')
}

/// A section title is one to six `=` characters, a space, and the text.
#[must_use]
pub fn title(line: &str, at: usize) -> Option<Title> {
    let level = line
        .chars()
        .take_while(|character| *character == '=')
        .count();
    if level == 0 || level > 6 {
        return None;
    }
    let text = line.get(level..)?.strip_prefix(' ')?.trim();
    if text.is_empty() {
        return None;
    }
    Some(Title {
        level,
        text: text.to_owned(),
        span: (at, at.saturating_add(line.len())),
    })
}

/// An anchor a document declares outright, on its own line, in either spelling.
#[must_use]
pub fn declared_anchor(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let inside = trimmed
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .or_else(|| {
            trimmed
                .strip_prefix("[#")
                .and_then(|rest| rest.strip_suffix(']'))
        })?;
    let id = inside.split(',').next().unwrap_or_default().trim();
    (!id.is_empty() && !id.contains(char::is_whitespace)).then(|| id.to_owned())
}
