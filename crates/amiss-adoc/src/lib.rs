use std::collections::BTreeMap;

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
    /// The document attributes its own entries define so far, by lowercased
    /// name, each with whether its value names one the document does not.
    attributes: BTreeMap<String, (String, bool)>,
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

/// One section title, with the level its `=` run declares, and whether its
/// text names an attribute the document does not define, which leaves its
/// identity to whatever defines that attribute when the site is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Title {
    pub level: usize,
    pub text: String,
    pub span: (usize, usize),
    pub unresolved: bool,
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
                text: macros::without_anchors(line),
                span: (at, at.saturating_add(line.len())),
                unresolved: false,
            });
        underline = setext.is_some();
        if let Some((name, value)) = attribute_entry(bare) {
            match value {
                Some(value) => {
                    let defined = substituted(value, &extraction.attributes);
                    extraction.attributes.insert(name, defined);
                }
                None => {
                    extraction.attributes.remove(&name);
                }
            }
        }
        if let Some(mut title) = macros::title(line, at).or(setext) {
            (title.text, title.unresolved) = substituted(&title.text, &extraction.attributes);
            extraction.anchors.extend(macros::declared_anchors(line));
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

/// A document attribute entry, `:name: value`, with its name lowercased the
/// way Asciidoctor stores it; `:name!:` and `:!name:` unset the attribute.
fn attribute_entry(line: &str) -> Option<(String, Option<&str>)> {
    let (name, value) = line.strip_prefix(':')?.split_once(':')?;
    let (name, unset) = match (name.strip_prefix('!'), name.strip_suffix('!')) {
        (Some(name), _) | (None, Some(name)) => (name, true),
        (None, None) => (name.strip_suffix('@').unwrap_or(name), false),
    };
    let named = name
        .chars()
        .next()
        .is_some_and(|first| first.is_alphanumeric() || first == '_')
        && name
            .chars()
            .all(|next| next.is_alphanumeric() || matches!(next, '_' | '-'));
    if !named || !(value.is_empty() || value.starts_with([' ', '\t'])) {
        return None;
    }
    let value = value.trim();
    Some((
        name.to_lowercase(),
        (!unset).then(|| value.strip_suffix(" \\").unwrap_or(value)),
    ))
}

/// Text with each attribute reference replaced the way Asciidoctor replaces
/// it: a defined attribute by its value, the empty built-ins by nothing or a
/// space, and an escaped one left as written. Whether a reference named an
/// attribute this document does not define comes back beside the text.
fn substituted(text: &str, attributes: &BTreeMap<String, (String, bool)>) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut unresolved = false;
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let (before, from) = rest.split_at(open);
        let Some(close) = from.find('}') else {
            break;
        };
        let name = from.get(1..close).unwrap_or_default();
        let escaped = before.ends_with('\\');
        out.push_str(
            before
                .strip_suffix('\\')
                .filter(|_| escaped)
                .unwrap_or(before),
        );
        let simple = !name.is_empty()
            && name
                .chars()
                .all(|next| next.is_alphanumeric() || matches!(next, '_' | '-'));
        let value = attributes
            .get(&name.to_lowercase())
            .map(|(value, inner)| (value.as_str(), *inner))
            .or(match name {
                "empty" | "blank" => Some(("", false)),
                "sp" => Some((" ", false)),
                _ => None,
            });
        match (escaped, value) {
            (false, Some((value, inner))) if simple => {
                unresolved |= inner;
                out.push_str(value);
            }
            _ => {
                unresolved |= !escaped && (simple || name.contains(':'));
                out.push_str(from.get(..=close).unwrap_or_default());
            }
        }
        rest = from.get(close.saturating_add(1)..).unwrap_or_default();
    }
    out.push_str(rest);
    (out, unresolved)
}

fn lines(body: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0_usize;
    body.split_inclusive('\n').map(move |line| {
        let at = offset;
        offset = offset.saturating_add(line.len());
        (at, line.strip_suffix('\n').unwrap_or(line))
    })
}
