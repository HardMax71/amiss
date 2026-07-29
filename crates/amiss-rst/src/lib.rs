pub mod block;
pub mod directive;

pub use block::{Block, Kind, blocks};
pub use directive::{Reference, ReferenceKind, references, target_definition, title_underline};

/// Everything one reStructuredText scan yields. The specification's own
/// reference vocabulary is small: hyperlink targets and four directives that
/// name a file. Roles are an open extension point, so an unregistered one is
/// declared rather than guessed at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub references: Vec<Reference>,
    pub titles: Vec<Title>,
    pub anchors: Vec<String>,
    pub opaque: Vec<(usize, usize)>,
    pub blocks: usize,
    pub nesting: usize,
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
            Kind::Literal | Kind::Comment => {
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
