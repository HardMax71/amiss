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
    Url,
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
            Self::Url => "asciidoc-url",
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
    if line.starts_with("include::")
        && let Some((target, options, end)) =
            bracketed(line, "include::".len(), &mut brackets(line))
    {
        let mut reference = build(ReferenceKind::Include, target, at, 0, end);
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
    let mut brackets = brackets(line);
    let mut xref_close = cursor(line, |rest| rest.find(">>"));
    let mut angle = cursor(line, |rest| rest.find('<'));
    let mut index = 0;
    while index < line.len() {
        if skipped(&skips, index) || bytes.get(index.wrapping_sub(1)) == Some(&b'\\') {
            index = index.saturating_add(1);
            continue;
        }
        if let Some(reference) = internal(line, at, index, &mut xref_close, &mut angle) {
            index = reference.span.1.saturating_sub(at);
            found.push(reference);
            continue;
        }
        if let Some(reference) = macro_at(line, at, index, &mut brackets)
            .or_else(|| url_at(line, at, index, &mut brackets))
        {
            index = reference.span.1.saturating_sub(at);
            found.push(reference);
            continue;
        }
        index = index.saturating_add(1);
    }
    found
}

/// The schemes Asciidoctor links when a URL is written directly, and the one
/// it links only with an attribute list after it.
const URL_SCHEMES: [&str; 4] = ["https://", "http://", "ftp://", "irc://"];
const MAILTO: &str = "mailto:";

/// A URL written directly: with an attribute list, `https://host[text]`, bare,
/// or between angle brackets, which Asciidoctor links as it reads. A bare
/// URL ends at whitespace or a bracket, and the trailing punctuation that
/// closes a sentence is not part of it.
fn url_at(line: &str, at: usize, index: usize, brackets: &mut Brackets<'_>) -> Option<Reference> {
    let tail = line.get(index..)?;
    let mailto = tail.starts_with(MAILTO);
    let scheme = URL_SCHEMES.iter().find(|scheme| tail.starts_with(**scheme));
    if !mailto && scheme.is_none() || !boundary(line, index) {
        return None;
    }
    let length = tail
        .find(|character: char| {
            character.is_whitespace() || matches!(character, '[' | '<' | '>' | '"' | '`')
        })
        .unwrap_or(tail.len());
    if tail.get(length..).is_some_and(|rest| rest.starts_with('[')) {
        let (target, _text, end) = bracketed(line, index, brackets)?;
        return Some(build(ReferenceKind::Url, target, at, index, end));
    }
    let scheme = scheme?;
    let target = tail
        .get(..length)?
        .trim_end_matches(['.', ',', ';', ':', '!', '?', ')']);
    (target.len() > scheme.len()).then(|| {
        build(
            ReferenceKind::Url,
            target,
            at,
            index,
            index.saturating_add(target.len()),
        )
    })
}

fn macro_at(line: &str, at: usize, index: usize, brackets: &mut Brackets<'_>) -> Option<Reference> {
    if !boundary(line, index) {
        return None;
    }
    let tail = line.get(index..)?;
    let (name, kind) = MACROS
        .iter()
        .find(|(name, _)| tail.starts_with(name))
        .copied()?;
    let (target, _options, end) = bracketed(line, index.checked_add(name.len())?, brackets)?;
    Some(build(kind, target, at, index, end))
}

/// A cross reference, `<<target,text>>`, closing on the first `>>` after it
/// and holding no `<` of its own.
fn internal(
    line: &str,
    at: usize,
    index: usize,
    xref_close: &mut Next<'_>,
    angle: &mut Next<'_>,
) -> Option<Reference> {
    line.get(index..)?.strip_prefix("<<")?;
    let from = index.checked_add(2)?;
    let close = next_at(xref_close, from)?;
    if close == from || next_at(angle, from).is_some_and(|inner| inner < close) {
        return None;
    }
    let inside = line.get(from..close)?;
    let target = inside.split(',').next().unwrap_or_default().trim();
    if target.is_empty() {
        return None;
    }
    Some(build(
        ReferenceKind::InternalCrossReference,
        target,
        at,
        index,
        close.checked_add(2)?,
    ))
}

/// Where one pattern next matches at or after a position, for a scan whose
/// positions only grow. Each match is found once however many positions ask
/// for it, which keeps one long line linear rather than quadratic.
struct Next<'a> {
    line: &'a str,
    pattern: fn(&str) -> Option<usize>,
    found: Option<usize>,
    exhausted: bool,
}

fn cursor(line: &str, pattern: fn(&str) -> Option<usize>) -> Next<'_> {
    Next {
        line,
        pattern,
        found: None,
        exhausted: false,
    }
}

fn next_at(cursor: &mut Next<'_>, position: usize) -> Option<usize> {
    if let Some(found) = cursor.found.filter(|found| *found >= position) {
        return Some(found);
    }
    if cursor.exhausted {
        return None;
    }
    let start = (position..=cursor.line.len()).find(|at| cursor.line.is_char_boundary(*at))?;
    let found = cursor
        .line
        .get(start..)
        .and_then(cursor.pattern)
        .and_then(|offset| offset.checked_add(start));
    cursor.exhausted = found.is_none();
    cursor.found = found;
    found
}

/// The three places a macro's attribute list is read off: the opening
/// bracket, the first whitespace, which ends a target before any bracket, and
/// the closing bracket.
struct Brackets<'a> {
    open: Next<'a>,
    space: Next<'a>,
    close: Next<'a>,
}

fn brackets(line: &str) -> Brackets<'_> {
    Brackets {
        open: cursor(line, |rest| rest.find('[')),
        space: cursor(line, |rest| rest.find(char::is_whitespace)),
        close: cursor(line, |rest| rest.find(']')),
    }
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

/// A macro target runs from where its name ends to the opening bracket of its
/// attribute list, and the list to its closing bracket, whose end is the
/// macro's. Whitespace before that bracket means this was prose that happened
/// to start with the macro name.
fn bracketed<'a>(
    line: &'a str,
    from: usize,
    brackets: &mut Brackets<'_>,
) -> Option<(&'a str, &'a str, usize)> {
    let open = next_at(&mut brackets.open, from)?;
    if open == from || next_at(&mut brackets.space, from).is_some_and(|space| space < open) {
        return None;
    }
    let close = next_at(&mut brackets.close, open)?;
    Some((
        line.get(from..open)?,
        line.get(open.checked_add(1)?..close)?,
        close.checked_add(1)?,
    ))
}

/// Whether a position falls inside one of the sorted, disjoint spans.
fn skipped(spans: &[(usize, usize)], position: usize) -> bool {
    spans
        .partition_point(|(start, _)| *start <= position)
        .checked_sub(1)
        .and_then(|last| spans.get(last))
        .is_some_and(|(_, end)| position < *end)
}

/// The byte intervals a macro name cannot start in: the inline passthroughs,
/// `+++text+++`, `$$text$$`, `++text++`, `+text+` and `pass:[text]`, which is
/// where a document quoting `AsciiDoc` syntax puts it. Monospace alone hides
/// nothing, since Asciidoctor still reads a macro written between backticks.
fn passthrough_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut brackets = brackets(line);
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
                next_at(&mut brackets.open, index.saturating_add("pass:".len()))
                    .and_then(|open| next_at(&mut brackets.close, open))
                    .and_then(|close| close.checked_add(1)?.checked_sub(index))
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
        let opened = !skipped(&skips, at)
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
    let mut brackets = brackets(line);
    found.extend(
        line.match_indices("anchor:")
            .filter(|(at, _)| boundary(line, *at) && !skipped(&skips, *at))
            .filter_map(|(at, name)| bracketed(line, at.saturating_add(name.len()), &mut brackets))
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
