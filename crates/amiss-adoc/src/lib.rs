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

/// Where a document's image macros resolve from. Asciidoctor joins an image
/// target to `imagesdir`, which is empty unless set, so an unset document
/// reads its images beside itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ImagesDir {
    #[default]
    Beside,
    Under(String),
    Unknown,
}

/// The `imagesdir` a document sets for all of itself: none, or one literal
/// directory among the header's attribute entries. An entry anywhere else, a
/// second one, an unset, or a value naming an attribute, a URL or an absolute
/// path leaves every image undecided, since the value in force at an image
/// then depends on more than this file says.
#[must_use]
pub fn images_dir(source: &[u8]) -> ImagesDir {
    const SET: &str = ":imagesdir:";
    let Ok(text) = std::str::from_utf8(source) else {
        return ImagesDir::Unknown;
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().skip_while(|line| {
        line.trim().is_empty() || (line.starts_with("//") && !line.starts_with("////"))
    });
    let header: Vec<&str> = lines
        .by_ref()
        .take_while(|line| !line.trim().is_empty())
        .collect();
    let opens = header
        .first()
        .is_some_and(|line| line.starts_with("= ") || line.starts_with(':'));
    let entries = |line: &str| {
        [SET, ":imagesdir!:", ":!imagesdir:"]
            .iter()
            .any(|entry| line.starts_with(entry))
    };
    let set: Vec<&str> = header
        .iter()
        .copied()
        .filter(|line| entries(line))
        .collect();
    if lines.any(entries) || (!opens && !set.is_empty()) {
        return ImagesDir::Unknown;
    }
    let [only] = set.as_slice() else {
        return if set.is_empty() {
            ImagesDir::Beside
        } else {
            ImagesDir::Unknown
        };
    };
    let Some(value) = only.strip_prefix(SET).map(str::trim) else {
        return ImagesDir::Unknown;
    };
    if value.contains('{') || value.contains("://") || value.starts_with('/') {
        return ImagesDir::Unknown;
    }
    let value = value.trim_end_matches('/');
    if value.is_empty() || value == "." {
        ImagesDir::Beside
    } else {
        ImagesDir::Under(value.to_owned())
    }
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
        if let Some(title) = macros::title(line, at) {
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
