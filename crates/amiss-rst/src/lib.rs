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

/// The reference forms the specification defines, plus the two Sphinx roles
/// every Sphinx project writes. `:doc:` and `:ref:` are modelled by name and
/// the grammar profile says so; every other role stays an open extension
/// point, declared rather than guessed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    InlineHyperlink,
    NamedTarget,
    Image,
    Include,
    FileOption,
    DocRole,
    RefRole,
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
            Self::RefRole => "rst-ref-role",
        }
    }

    #[must_use]
    pub const fn is_image(self) -> bool {
        matches!(self, Self::Image)
    }
}

/// One recognised reference, with the exact source text of its target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub kind: ReferenceKind,
    pub target: String,
    pub span: (usize, usize),
    pub block: usize,
    pub block_span: (usize, usize),
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
/// the prose after it. Anything else stays an opaque comment.
fn carrier(body: &str) -> Option<(usize, (String, String, String))> {
    let (line, rest) = body
        .split_once('\n')
        .map_or((body, ""), |(first, tail)| (first, tail));
    if !rest.bytes().all(|byte| byte == b'\n' || byte == b'\r') {
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
            Kind::Comment => {
                match carrier(body) {
                    Some((line_length, (label, url, title))) => {
                        extraction.governed.push(GovernedCarrier {
                            span: (block.span.0, block.span.0.saturating_add(line_length)),
                            label,
                            url,
                            title,
                        });
                    }
                    None => extraction.opaque.push(block.span),
                }
                continue;
            }
            Kind::Literal => {
                extraction.opaque.push(block.span);
                continue;
            }
            Kind::Directive => {
                collect(&mut extraction, index, block, body);
                continue;
            }
            Kind::Text => {}
        }
        read_titles(&mut extraction, &mut order, block, body);
        collect(&mut extraction, index, block, body);
    }
    Ok(extraction)
}

fn read_titles(extraction: &mut Extraction, order: &mut Vec<char>, block: &Block, body: &str) {
    let mut offset = 0_usize;
    let mut previous: Option<(usize, &str)> = None;
    for raw in body.split_inclusive('\n') {
        let at = offset;
        offset = offset.saturating_add(raw.len());
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        if let Some((text_at, text)) = previous
            && let Some(character) = title_underline(line, text)
        {
            let level = order
                .iter()
                .position(|held| *held == character)
                .map_or_else(
                    || {
                        order.push(character);
                        order.len()
                    },
                    |found| found.saturating_add(1),
                );
            extraction.titles.push(Title {
                level,
                text: text.trim().to_owned(),
                span: (
                    block.span.0.saturating_add(text_at),
                    block.span.0.saturating_add(offset),
                ),
            });
            previous = None;
            continue;
        }
        previous = (!line.trim().is_empty()).then_some((at, line));
    }
}

fn collect(extraction: &mut Extraction, index: usize, block: &Block, body: &str) {
    let mut offset = 0_usize;
    for raw in body.split_inclusive('\n') {
        let at = block.span.0.saturating_add(offset);
        offset = offset.saturating_add(raw.len());
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        if let Some(label) = target_definition(line) {
            extraction.anchors.push(label);
        }
        for mut reference in references(line, at) {
            reference.block = index;
            reference.block_span = block.span;
            extraction.references.push(reference);
        }
    }
}
