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
    let skips = passthrough_spans(line);
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

/// The byte intervals a macro name cannot start in: the inline passthroughs,
/// `+++text+++`, `$$text$$`, `++text++`, `+text+` and `pass:[text]`, which is
/// where a document quoting `AsciiDoc` syntax puts it. Monospace alone hides
/// nothing, since Asciidoctor still reads a macro written between backticks.
fn passthrough_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0;
    while let Some(tail) = line.get(index..) {
        let Some(first) = tail.chars().next() else {
            break;
        };
        let fence = PASSTHROUGH_FENCES
            .iter()
            .find(|fence| tail.starts_with(**fence));
        let closed = match fence {
            Some(fence) => tail
                .get(fence.len()..)
                .and_then(|rest| rest.find(fence))
                .map(|close| close.saturating_add(fence.len().saturating_mul(2))),
            None if tail.starts_with("pass:") && boundary(line, index) => {
                tail.find('[').and_then(|open| {
                    let close = tail.get(open..)?.find(']')?;
                    open.checked_add(close)?.checked_add(1)
                })
            }
            None => None,
        };
        match closed {
            Some(length) => {
                spans.push((index, index.saturating_add(length)));
                index = index.saturating_add(length);
            }
            None => index = index.saturating_add(first.len_utf8()),
        }
    }
    spans
}

const PASSTHROUGH_FENCES: [&str; 4] = ["+++", "$$", "++", "+"];

/// A macro name only opens a macro at the start of a word. Without this,
/// prose ending in a word that happens to close with the name would open one.
fn boundary(line: &str, index: usize) -> bool {
    line.get(..index)
        .and_then(|before| before.chars().next_back())
        .is_none_or(|character| !character.is_alphanumeric() && character != '_')
}

/// A section title is one to six `=` characters, or the `#` characters
/// Asciidoctor accepts from Markdown, a space, and the text. An anchor written
/// into the title is an identity of its own, so the text a generated identity
/// is made from leaves it out.
#[must_use]
pub fn title(line: &str, at: usize) -> Option<Title> {
    let marker = line
        .chars()
        .next()
        .filter(|first| matches!(first, '=' | '#'))?;
    let level = line
        .chars()
        .take_while(|character| *character == marker)
        .count();
    if level > 6 {
        return None;
    }
    let text = without_anchors(line.get(level..)?.strip_prefix(' ')?);
    if text.is_empty() {
        return None;
    }
    Some(Title {
        level,
        text,
        span: (at, at.saturating_add(line.len())),
    })
}

/// Text with every `[[id]]` anchor taken out and trimmed, which is what
/// Asciidoctor makes a section's generated identity from.
#[must_use]
pub fn without_anchors(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest.get(open..).and_then(|tail| tail.find("]]")) else {
            break;
        };
        kept.push_str(rest.get(..open).unwrap_or_default());
        rest = rest
            .get(open.saturating_add(close).saturating_add(2)..)
            .unwrap_or_default();
    }
    kept.push_str(rest);
    kept.trim().to_owned()
}

/// Every identity one line declares: what a line standing alone as a block
/// anchor, `[[id,reftext]]`, or as an attribute list names, or else each
/// anchor, bibliography anchor and `anchor:` macro written in the flow of its
/// text, with the reference text any of them carries where a natural cross
/// reference could name it.
#[must_use]
pub fn declared_anchors(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let alone = match trimmed
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .filter(|inside| !inside.contains('['))
    {
        Some(inside) => named(Some(inside)),
        None => trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .map(attribute_ids)
            .unwrap_or_default(),
    };
    if !alone.is_empty() {
        return alone;
    }
    let skips = passthrough_spans(line);
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(open) = line.get(index..).and_then(|tail| tail.find("[[")) {
        let at = index.saturating_add(open);
        let bibliography = line.get(at..).is_some_and(|tail| tail.starts_with("[[["));
        let (fence, closer) = if bibliography {
            ("[[[", "]]]")
        } else {
            ("[[", "]]")
        };
        index = at.saturating_add(fence.len());
        let opened = !skips.iter().any(|(start, end)| at >= *start && at < *end)
            && !matches!(
                line.get(..at).and_then(|before| before.chars().next_back()),
                Some('\\' | '[')
            );
        let Some(close) = line.get(index..).and_then(|tail| tail.find(closer)) else {
            break;
        };
        if opened {
            found.extend(named(line.get(index..index.saturating_add(close))));
        }
        index = index.saturating_add(close).saturating_add(closer.len());
    }
    found.extend(
        line.match_indices("anchor:")
            .filter(|(at, _)| {
                boundary(line, *at) && !skips.iter().any(|(start, end)| (*start..*end).contains(at))
            })
            .filter_map(|(at, name)| {
                line.get(at.saturating_add(name.len())..)
                    .and_then(target_of)
            })
            .flat_map(|(id, reftext, _end)| named(Some(&format!("{id},{reftext}")))),
    );
    found
}

/// An anchor's identity under Asciidoctor's own grammar, then the reference
/// text after its comma where a natural cross reference could name it.
fn named(inside: Option<&str>) -> Vec<String> {
    let Some(inside) = inside else {
        return Vec::new();
    };
    let (id, reftext) = inside.split_once(',').unwrap_or((inside, ""));
    anchor_id(Some(id))
        .into_iter()
        .chain(reference_text(reftext))
        .collect()
}

/// Reference text a natural cross reference names, which Asciidoctor reads as
/// text rather than as an identity only when it holds a space or a capital.
#[must_use]
pub fn reference_text(text: &str) -> Option<String> {
    let text = text.trim().trim_matches('"');
    (text.contains(' ') || text.chars().any(char::is_uppercase)).then(|| text.to_owned())
}

/// The identities one attribute list names: the `#id` shorthand in its first
/// positional attribute, and its named `id` and `reftext` attributes.
fn attribute_ids(inside: &str) -> Vec<String> {
    inside
        .split(',')
        .enumerate()
        .flat_map(|(position, attribute)| {
            let attribute = attribute.trim();
            let shorthand = attribute
                .split_once('#')
                .filter(|_split| position == 0)
                .and_then(|(_style, shorthand)| anchor_id(shorthand.split(['.', '%']).next()));
            let assigned = match attribute.split_once('=') {
                Some(("id", value)) => anchor_id(Some(value.trim().trim_matches('"'))),
                Some(("reftext", value)) => reference_text(value),
                Some(_) | None => None,
            };
            shorthand.into_iter().chain(assigned)
        })
        .collect()
}

/// Asciidoctor's own ID grammar, which the flow of text needs and a line
/// carrying nothing else does not: a letter, `_` or `:` opens it, and word
/// characters, `-`, `:` and `.` carry it on. Reference text after a comma
/// names no identity.
fn anchor_id(inside: Option<&str>) -> Option<String> {
    let id = inside?.split(',').next().unwrap_or_default().trim();
    let mut characters = id.chars();
    let opens = characters
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == ':');
    let carries = characters
        .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '.'));
    (opens && carries).then(|| id.to_owned())
}
