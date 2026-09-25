pub mod adapter;
pub mod block;
pub mod directive;

pub use adapter::analyze;

pub use block::blocks;
pub use directive::{references, target_definition, title_underline};

/// Everything one reStructuredText scan yields. The specification's own
/// reference vocabulary is small: hyperlink targets and four directives that
/// name a file. Roles are an open extension point, so an unregistered one is
/// declared rather than guessed at.
/// One recognized governed carrier: its span, then label, url, and title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedCarrier {
    pub span: (usize, usize),
    pub label: String,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub references: Vec<Reference>,
    pub titles: Vec<Title>,
    pub governed: Vec<GovernedCarrier>,
    pub anchors: Vec<String>,
    pub opaque: Vec<(usize, usize)>,
    pub blocks: usize,
    pub nesting: usize,
}

/// What a block holds. `Literal` is an indented literal block opened by `::`,
/// whose content is code. `Comment` is an explicit markup block this parser
/// declines to read into. `Directive` holds a directive's argument and options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Literal,
    Comment,
    Directive,
}

/// One block of a document: its byte span, what it holds, and the indent that
/// opened it, which is the only nesting reStructuredText has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub span: (usize, usize),
    pub kind: Kind,
    pub indent: usize,
}

/// The reference forms the specification defines, plus the Sphinx roles that
/// name a document, a file or a label. `:doc:`, `:download:`, `:ref:` and
/// `:numref:` are modelled by name and the grammar profile says so; every
/// other role stays an open extension point, declared rather than guessed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    InlineHyperlink,
    NamedTarget,
    Image,
    Include,
    FileOption,
    DocRole,
    DownloadRole,
    RefRole,
    NumrefRole,
    TermRole,
    TocTreeEntry,
    TargetOption,
}

impl ReferenceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InlineHyperlink => "rst-inline-hyperlink",
            Self::NamedTarget => "rst-named-target",
            Self::Image => "rst-image-directive",
            Self::Include => "rst-include-directive",
            Self::FileOption => "rst-file-option",
            Self::DocRole => "rst-doc-role",
            Self::DownloadRole => "rst-download-role",
            Self::RefRole => "rst-ref-role",
            Self::TermRole => "rst-term-role",
            Self::NumrefRole => "rst-numref-role",
            Self::TocTreeEntry => "rst-toctree-entry",
            Self::TargetOption => "rst-target-option",
        }
    }

    #[must_use]
    pub const fn is_image(self) -> bool {
        matches!(self, Self::Image)
    }
}

/// One recognised reference, with the exact source text of its target and the
/// part of that target a `literalinclude` option selects, spelled as the line
/// or object fragment the resolver reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub target: String,
    pub selection: Option<String>,
    pub span: (usize, usize),
    pub block: usize,
    pub block_span: (usize, usize),
    pub transclusion: Option<
        Result<
            amiss_wire::extraction::TransclusionKind,
            amiss_wire::extraction::TransclusionRefusal,
        >,
    >,
}

/// One section title. Its level comes from the order its underline character
/// first appears in the document, which is how the specification defines the
/// hierarchy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Title {
    pub level: usize,
    pub text: String,
    pub span: (usize, usize),
}

/// The Docutils simple-name normalization Sphinx stores labels under:
/// case-folded, with internal whitespace runs collapsed to one space.
#[must_use]
pub fn normalized_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The reserved governed carrier: a column-zero comment whose first line is
/// exactly the carrier line and whose remainder is blank. Returns the line's
/// byte length so the governed span excludes the terminator and the blank
/// tail, which is what keeps an applied fix from merging the comment into
/// the prose after it. Anything else stays an unread comment.
fn carrier(body: &str) -> Option<(usize, (String, String, String))> {
    let (line, rest) = body
        .split_once('\n')
        .map_or((body, ""), |(first, tail)| (first, tail));
    if !rest.trim().is_empty() {
        return None;
    }
    let line = line.strip_suffix('\r').unwrap_or(line);
    let parts = amiss_wire::extraction::governed_carrier_line(line.strip_prefix(".. ")?)?;
    Some((line.len(), parts))
}

/// The reasons a document is refused before anything is extracted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NotUtf8,
}

/// Scans one reStructuredText document.
///
/// # Errors
///
/// `NotUtf8` when the bytes are not valid UTF-8.
pub fn extract(source: &[u8]) -> Result<Extraction, Refusal> {
    let text = std::str::from_utf8(source).map_err(|_invalid| Refusal::NotUtf8)?;
    let scanned = blocks(text);
    let mut extraction = Extraction {
        blocks: scanned.len(),
        nesting: scanned.iter().map(|block| block.indent).max().unwrap_or(0),
        ..Extraction::default()
    };
    let mut order: Vec<char> = Vec::new();
    for (index, block) in scanned.iter().enumerate() {
        let body = text.get(block.span.0..block.span.1).unwrap_or_default();
        match block.kind {
            // A comment never renders and a literal block renders as code, so
            // neither is a blind spot; only injected raw output is opaque.
            Kind::Comment => {
                if let Some((line_length, (label, url, title))) = carrier(body) {
                    extraction.governed.push(GovernedCarrier {
                        span: (block.span.0, block.span.0.saturating_add(line_length)),
                        label,
                        url,
                        title,
                    });
                }
                continue;
            }
            Kind::Literal => continue,
            Kind::Directive => {
                if raw_directive(body) {
                    extraction.opaque.push(block.span);
                } else {
                    read_block(&mut extraction, None, index, block, body);
                }
                continue;
            }
            Kind::Text => {}
        }
        read_block(&mut extraction, Some(&mut order), index, block, body);
    }
    Ok(extraction)
}

fn read_block(
    extraction: &mut Extraction,
    mut title_order: Option<&mut Vec<char>>,
    index: usize,
    block: &Block,
    body: &str,
) {
    let has_directive_body = body.lines().skip(1).any(|line| !line.trim().is_empty());
    let literal_include = block.kind == Kind::Directive && opens_literal_include(body);
    let mut offset = 0_usize;
    let mut previous: Option<(usize, &str)> = None;
    let mut literal: Option<(usize, bool)> = None;
    let mut toctree: Option<usize> = None;
    let mut glossary: Option<Glossary> = None;
    let mut read_until = 0_usize;
    for raw in body.split_inclusive('\n') {
        let text_at = offset;
        let at = block.span.0.saturating_add(offset);
        offset = offset.saturating_add(raw.len());
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let indent = line.len().saturating_sub(line.trim_start().len());
        if let Some((opened, options)) = literal {
            let blank = line.trim().is_empty();
            if blank || indent > opened {
                let option = options && !blank && line.trim_start().starts_with(':');
                literal = Some((opened, option));
                if !option {
                    continue;
                }
            } else {
                literal = None;
            }
        }
        if let Some(opened) = toctree {
            if line.trim().is_empty() || indent > opened {
                if let Some(mut entry) = directive::toctree_entry(line, at) {
                    entry.block = index;
                    entry.block_span = block.span;
                    extraction.references.push(entry);
                }
                continue;
            }
            toctree = None;
        }
        if let Some(order) = title_order.as_deref_mut() {
            let title = previous.and_then(|(start, text)| {
                title_underline(line, text).map(|character| (start, text, character))
            });
            if let Some((start, text, character)) = title {
                extraction.titles.push(Title {
                    level: title_level(order, character),
                    text: text.trim().to_owned(),
                    span: (
                        block.span.0.saturating_add(start),
                        block.span.0.saturating_add(offset),
                    ),
                });
                previous = None;
            } else {
                previous = (!line.trim().is_empty()).then_some((text_at, line));
            }
        }
        declare(
            &mut extraction.anchors,
            &mut glossary,
            line,
            indent,
            block.kind == Kind::Directive,
        );
        let chunk = if text_at >= read_until {
            let joined = wrapped(body, text_at, line);
            read_until = text_at.saturating_add(joined.len());
            joined
        } else {
            ""
        };
        if let Some(selection) = include_selection(line).filter(|_| literal_include)
            && let Some(last) = extraction
                .references
                .last_mut()
                .filter(|last| last.block == index)
        {
            last.selection = Some(selection);
        }
        for mut reference in references(chunk, at) {
            reference.block = index;
            reference.block_span = block.span;
            if let Some(mode) = reference.transclusion {
                reference.transclusion = Some(if block.indent != 0 {
                    Err(amiss_wire::extraction::TransclusionRefusal::Context)
                } else if has_directive_body {
                    Err(amiss_wire::extraction::TransclusionRefusal::Options)
                } else {
                    mode
                });
            }
            extraction.references.push(reference);
        }
        if literal.is_none() {
            literal = literal_opener(line).map(|options| (indent, options));
        }
        if directive_name(line.trim_start()).is_some_and(|name| name == "toctree") {
            toctree = Some(indent);
        }
    }
}

/// The names one line declares: a `.. _label:` target, a directive's `:name:`
/// option, and a glossary term, with the glossary a line opens read on the
/// lines after it.
fn declare(
    anchors: &mut Vec<String>,
    glossary: &mut Option<Glossary>,
    line: &str,
    indent: usize,
    in_directive: bool,
) {
    anchors.extend(target_definition(line));
    if in_directive {
        anchors.extend(amiss_wire::extraction::directive_name_option(line));
    }
    anchors.extend(glossary_term(glossary, line, indent));
    if directive_name(line.trim_start()) == Some("glossary") {
        *glossary = Some(Glossary {
            opened: indent,
            terms_at: None,
        });
    }
}

/// A `glossary` body being read: the opener's indent, and the indent its
/// first term set, which every later term shares.
#[derive(Clone, Copy)]
struct Glossary {
    opened: usize,
    terms_at: Option<usize>,
}

/// The term one line of a glossary body declares. Options come before the
/// first term, a definition sits deeper than the terms, and a line back at the
/// opener's indent ends the body.
fn glossary_term(glossary: &mut Option<Glossary>, line: &str, indent: usize) -> Option<String> {
    let state = (*glossary)?;
    if line.trim().is_empty() {
        return None;
    }
    if indent <= state.opened {
        *glossary = None;
        return None;
    }
    if state.terms_at.is_none() && line.trim_start().starts_with(':') {
        return None;
    }
    let terms_at = state.terms_at.unwrap_or(indent);
    *glossary = Some(Glossary {
        opened: state.opened,
        terms_at: Some(terms_at),
    });
    // A term may carry classifiers after ` : `, which name no part of it.
    (indent == terms_at).then(|| {
        line.trim()
            .split(" : ")
            .next()
            .unwrap_or_default()
            .replace('`', "")
    })
}

/// The line, or when its backticks do not balance, the line and as many of the
/// following lines as close the role or link it opens, up to a blank line or
/// eight lines in all. Docutils reads inline markup across line breaks.
fn wrapped<'body>(body: &'body str, from: usize, line: &'body str) -> &'body str {
    let rest = body.get(from..).unwrap_or_default();
    let mut ticks = 0_usize;
    let mut end = 0_usize;
    for (taken, raw) in rest.split_inclusive('\n').take(8).enumerate() {
        if taken > 0 && (ticks.is_multiple_of(2) || raw.trim().is_empty()) {
            break;
        }
        ticks = ticks.saturating_add(raw.matches('`').count());
        end = end.saturating_add(raw.len());
    }
    let extended = end > line.len().saturating_add(1);
    if extended && ticks.is_multiple_of(2) {
        rest.get(..end).unwrap_or(line)
    } else {
        line
    }
}

/// The directives whose body is code, output, or diagram source rather than
/// reStructuredText; Docutils and Sphinx render it as it is written.
const LITERAL_DIRECTIVES: [&str; 17] = [
    "code",
    "code-block",
    "sourcecode",
    "doctest",
    "testcode",
    "testoutput",
    "testsetup",
    "testcleanup",
    "ipython",
    "jupyter-execute",
    "math",
    "graphviz",
    "graph",
    "digraph",
    "uml",
    "mermaid",
    "prompt",
];

/// Whether a line opens a literal body: a literal directive, whose option
/// lines still count, or a paragraph ending in `::`, whose indented
/// continuation is a literal block wherever it sits.
fn literal_opener(line: &str) -> Option<bool> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("..") {
        return directive_name(trimmed)
            .is_some_and(|name| {
                LITERAL_DIRECTIVES
                    .iter()
                    .any(|literal| name.eq_ignore_ascii_case(literal))
            })
            .then_some(true);
    }
    trimmed.trim_end().ends_with("::").then_some(false)
}

/// The name of the directive a line opens, `code-block` for `.. code-block::`.
/// The level a title's underline character takes: its place in the order the
/// document first used each character, recorded the first time it is seen.
fn title_level(order: &mut Vec<char>, character: char) -> usize {
    if let Some(found) = order.iter().position(|held| *held == character) {
        return found.saturating_add(1);
    }
    order.push(character);
    order.len()
}

/// Whether a directive block opens a `literalinclude`, the directive whose
/// options select part of a file.
fn opens_literal_include(body: &str) -> bool {
    body.lines()
        .next()
        .and_then(|first| directive_name(first.trim_start()))
        == Some("literalinclude")
}

/// The part of a file a `literalinclude` option selects, as a fragment: a
/// `:lines:` spec as the span from its first to its last selected line,
/// `L5-L8`, or `L5` where it runs on to the end, and a `:pyobject:` as the
/// object's name.
fn include_selection(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(name) = trimmed.strip_prefix(":pyobject:").map(str::trim) {
        return (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_owned());
    }
    let mut first: Option<u64> = None;
    let mut last: Option<u64> = Some(0);
    for part in trimmed.strip_prefix(":lines:")?.split(',') {
        let (start, end) = part.split_once('-').unwrap_or((part, part));
        let start = match start.trim() {
            "" => 1,
            written => written.parse().ok()?,
        };
        let end = match end.trim() {
            "" => None,
            written => Some(written.parse::<u64>().ok()?),
        };
        first = Some(first.map_or(start, |held| held.min(start)));
        last = last.zip(end).map(|(held, end)| held.max(end));
    }
    let first = first?;
    Some(match last.filter(|last| *last > first) {
        Some(last) => format!("L{first}-L{last}"),
        None => format!("L{first}"),
    })
}

fn directive_name(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix(".. ")
        .and_then(|rest| rest.split_once("::"))
        .map(|(name, _argument)| name.trim())
}

/// A `raw` directive injects its body into the output verbatim: rendered
/// content the parser cannot read, which is what opaque means. A nested
/// directive keeps its indent in the body, so the marker is read past it.
fn raw_directive(body: &str) -> bool {
    directive_name(body.split('\n').next().unwrap_or_default().trim_start())
        .is_some_and(|name| name.eq_ignore_ascii_case("raw"))
}
