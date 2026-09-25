pub mod adapter;
pub mod block;
pub mod macros;

pub use adapter::analyze;

pub use block::blocks;
pub use macros::{Reference, ReferenceKind, references};

/// Everything one `AsciiDoc` scan yields: the references it recognised, the
/// section titles that carry anchor identity, the identities a document
/// declares outright or publishes as its own reference text, and the byte
/// intervals it refused to read into.
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
    pub anchors: Vec<String>,
    pub governed: Vec<GovernedCarrier>,
    pub opaque: Vec<(usize, usize)>,
    pub blocks: usize,
    pub nesting: usize,
}

/// What a block does to the text inside it. `Verbatim` is listing and
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

/// One block of a document: its byte span, what it does to the text inside it,
/// which a delimiter line or an indented first line fixes, how deeply it
/// nests, and whether its first line carries a list marker.
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
            Some(delimiter @ (Delimiter::Passthrough | Delimiter::Verbatim)) => {
                if delimiter == Delimiter::Passthrough {
                    extraction.opaque.push(block.span);
                }
                let body = text.get(block.span.0..block.span.1).unwrap_or_default();
                verbatim_includes(&mut extraction, index, block, body);
                continue;
            }
            Some(Delimiter::Comment | Delimiter::Compound) => continue,
            None => {}
        }
        let body = text.get(block.span.0..block.span.1).unwrap_or_default();
        collect(&mut extraction, index, block, body);
    }
    Ok(extraction)
}

/// Asciidoctor resolves an include before it parses blocks, so one inside a
/// listing, literal or passthrough block still names a file, whose content
/// lands as literal text rather than as parsed `AsciiDoc`.
fn verbatim_includes(extraction: &mut Extraction, index: usize, block: &Block, body: &str) {
    for (offset, line) in lines(body) {
        if !line.starts_with("include::") {
            continue;
        }
        let at = block.span.0.saturating_add(offset);
        for mut reference in references(line, at) {
            if reference.kind != ReferenceKind::Include {
                continue;
            }
            reference.block = index;
            reference.block_span = block.span;
            reference.transclusion =
                Some(Err(amiss_wire::extraction::TransclusionRefusal::Context));
            extraction.references.push(reference);
        }
    }
}

fn collect(extraction: &mut Extraction, index: usize, block: &Block, body: &str) {
    let rows: Vec<(usize, &str)> = lines(body).collect();
    let mut underline = false;
    for (row, &(offset, line)) in rows.iter().enumerate() {
        if underline {
            underline = false;
            continue;
        }
        let at = block.span.0.saturating_add(offset);
        let bare = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = bare.strip_prefix("// ")
            && let Some(parts) = amiss_wire::extraction::governed_carrier_line(rest)
        {
            let (label, url, title) = parts;
            extraction.governed.push(GovernedCarrier {
                span: (at, at.saturating_add(bare.len())),
                label,
                url,
                title,
            });
            continue;
        }
        // Asciidoctor drops a line opening with `//` and no third slash.
        if bare
            .strip_prefix("//")
            .is_some_and(|rest| !rest.starts_with('/'))
        {
            continue;
        }
        let setext = rows
            .get(row.saturating_add(1))
            .and_then(|(_offset, next)| block::setext_level(line, next))
            .map(|level| Title {
                level,
                text: line.trim().to_owned(),
                span: (at, at.saturating_add(line.len())),
            });
        underline = setext.is_some();
        if let Some(title) = macros::title(line, at).or(setext) {
            // Asciidoctor names a title by its text only when it holds a space or a capital.
            if title.text.contains(' ') || title.text.chars().any(char::is_uppercase) {
                extraction.anchors.push(title.text.clone());
            }
            extraction.titles.push(title);
            continue;
        }
        extraction.anchors.extend(macros::declared_anchors(line));
        for mut reference in references(line, at) {
            reference.block = index;
            reference.block_span = block.span;
            reference.list_item = block.list_item;
            if reference.transclusion.is_some() && (block.depth != 0 || block.list_item) {
                reference.transclusion =
                    Some(Err(amiss_wire::extraction::TransclusionRefusal::Context));
            }
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
