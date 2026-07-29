pub mod adapter;
pub mod block;
pub mod macros;

pub use adapter::analyze;

pub use block::blocks;
pub use macros::{Reference, ReferenceKind, references};

/// Everything one `AsciiDoc` scan yields: the references it recognised, the
/// section titles that carry anchor identity, the explicit anchors a document
/// declares, and the byte intervals it refused to read into.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub references: Vec<Reference>,
    pub titles: Vec<Title>,
    pub anchors: Vec<String>,
    pub opaque: Vec<(usize, usize)>,
    pub blocks: usize,
    pub nesting: usize,
}

/// What a delimited block does to the text inside it. `Verbatim` is listing and
/// literal, whose content is code rather than prose. `Passthrough` and `Comment`
/// are refused outright and declared. `Compound` is a container whose own
/// paragraphs are separate blocks, so it is never read directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Verbatim,
    Passthrough,
    Comment,
    Compound,
}

/// One block of a document: its byte span, the delimiter that opened it if any,
/// how deeply it nests, and whether its first line carries a list marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub span: (usize, usize),
    pub delimiter: Option<Delimiter>,
    pub depth: usize,
    pub list_item: bool,
}

/// One section title, with the level its `=` run declares.
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

/// Scans one `AsciiDoc` document.
///
/// # Errors
///
/// `NotUtf8` when the bytes are not valid UTF-8; `AsciiDoc` is text.
pub fn extract(source: &[u8]) -> Result<Extraction, Refusal> {
    let text = std::str::from_utf8(source).map_err(|_invalid| Refusal::NotUtf8)?;
    let scanned = blocks(text);
    let mut extraction = Extraction {
        blocks: scanned.len(),
        nesting: scanned.iter().map(|block| block.depth).max().unwrap_or(0),
        ..Extraction::default()
    };
    for (index, block) in scanned.iter().enumerate() {
        match block.delimiter {
            Some(Delimiter::Passthrough) => {
                extraction.opaque.push(block.span);
                continue;
            }
            Some(Delimiter::Comment | Delimiter::Compound | Delimiter::Verbatim) => continue,
            None => {}
        }
        let body = text.get(block.span.0..block.span.1).unwrap_or_default();
        collect(&mut extraction, index, block, body);
    }
    Ok(extraction)
}

fn collect(extraction: &mut Extraction, index: usize, block: &Block, body: &str) {
    for (offset, line) in lines(body) {
        let at = block.span.0.saturating_add(offset);
        if let Some(title) = macros::title(line, at) {
            extraction.titles.push(title);
            continue;
        }
        if let Some(anchor) = macros::declared_anchor(line) {
            extraction.anchors.push(anchor);
            continue;
        }
        for mut reference in references(line, at) {
            reference.block = index;
            reference.block_span = block.span;
            reference.list_item = block.list_item;
            extraction.references.push(reference);
        }
    }
}

fn lines(body: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0_usize;
    body.split_inclusive('\n').map(move |line| {
        let at = offset;
        offset = offset.saturating_add(line.len());
        (at, line.strip_suffix('\n').unwrap_or(line))
    })
}
