mod tests;

use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::controls::SourceConstruct;
pub use amiss_wire::extraction::{
    Analysis, AnalyzeError, BlockKind, Extraction, Fault, GovernedDefinition, Heading,
    HeadingAttribute, HeadingSource, Occurrence, Opaque, Work,
};
use amiss_wire::model::Adapter;
use markdown::mdast::{Node, ReferenceKind};

use crate::accounting::{parsed, plain, walk};
use crate::frontmatter;

/// Charges and extracts one document in a single guarded parse. The lexical
/// rescans of embedded code stay inside `embedded_code_allowance`: every ask
/// is charged before it is scanned, so a crossing ends the parse with the
/// rejected ask charged but never read, and the spent total an
/// `EmbeddedCodeAllowance` error reports may exceed the allowance by that one
/// ask.
///
/// # Errors
///
/// `DocumentInvalid` for non-UTF-8 bytes or a grammar rejection under a
/// parsing adapter, `ParserPanic` when the parser panics, `ParserError` when
/// the returned tree breaks the parser's own contract, `InvalidSourceSpan`
/// when a span violates the closed source contract, and
/// `EmbeddedCodeAllowance` when the meter ends the parse.
pub fn analyze(
    adapter: Adapter,
    source: &[u8],
    embedded_code_allowance: u64,
) -> Result<Analysis, AnalyzeError> {
    let Some((tree, offset, suffix, embedded_code_bytes)) =
        parsed(adapter, source, embedded_code_allowance)?
    else {
        return Ok(Analysis {
            work: plain(source),
            embedded_code_bytes: 0,
            extraction: None,
        });
    };
    let frontmatter_bytes = frontmatter::recognize(source).map_or(0, |region| region.bytes);
    let extraction = extract_tree(&tree, suffix, offset, source, frontmatter_bytes)?;
    Ok(Analysis {
        work: walk(&tree),
        embedded_code_bytes,
        extraction: Some(extraction),
    })
}

#[derive(Clone, Copy, Default)]
struct Owners {
    list_item: Option<(usize, usize)>,
    cell: Option<(usize, usize)>,
    paragraph: Option<(usize, usize)>,
}

struct Definition {
    identifier: String,
    url: String,
    raw: String,
    reserved: bool,
}

/// A definition is reserved exactly when its decoded label scalars, before
/// `CommonMark` whitespace and case normalization, begin with lowercase ASCII
/// `amiss:`.
pub const RESERVED_LABEL_PREFIX: &str = "amiss:";

fn extract_tree(
    tree: &Node,
    suffix: &str,
    offset: usize,
    raw: &[u8],
    frontmatter_bytes: usize,
) -> Result<Extraction, Fault> {
    let (resolved, governed, orphans) = definitions(tree, suffix)?;
    let mut sweep = Sweep {
        suffix,
        definitions: resolved,
        orphans,
        root_span: span_of(tree)?,
        occurrences: Vec::new(),
        headings: Vec::new(),
        declared: Vec::new(),
        mdx: Vec::new(),
        html: Vec::new(),
    };
    let mut stack: Vec<(&Node, Vec<usize>, Owners)> = vec![(tree, Vec::new(), Owners::default())];
    while let Some((node, path, mut owners)) = stack.pop() {
        if !sweep.visit(node, &path, &mut owners)? {
            continue;
        }
        if let Some(children) = node.children() {
            for (index, child) in children.iter().enumerate().rev() {
                let mut child_path = path.clone();
                child_path.push(index);
                stack.push((child, child_path, owners));
            }
        }
    }

    sweep.occurrences.sort_by(|left, right| {
        left.span
            .cmp(&right.span)
            .then(left.node_path.cmp(&right.node_path))
    });
    let opaque = Opaque {
        frontmatter_bytes,
        mdx: union(sweep.mdx),
        html: union(sweep.html),
    };
    sweep.headings.extend(html_headings(suffix, &opaque.html));
    sweep.headings.sort_by_key(|heading| heading.span);
    validate(
        &sweep.occurrences,
        &sweep.headings,
        &opaque,
        offset,
        suffix.len(),
        raw,
    )?;
    let html_anchors = html_anchors(suffix, &opaque.html);

    let translate =
        |span: (usize, usize)| (span.0.saturating_add(offset), span.1.saturating_add(offset));
    Ok(Extraction {
        occurrences: sweep
            .occurrences
            .into_iter()
            .map(|entry| Occurrence {
                span: translate(entry.span),
                block_span: translate(entry.block_span),
                fragment_span: gated_span(
                    amiss_wire::extraction::fragment_span,
                    suffix.as_bytes(),
                    entry.span,
                    &entry.raw_destination,
                    entry.construct,
                )
                .map(translate),
                path_span: gated_span(
                    amiss_wire::extraction::path_span,
                    suffix.as_bytes(),
                    entry.span,
                    &entry.raw_destination,
                    entry.construct,
                )
                .map(translate),
                ..entry
            })
            .collect(),
        opaque: Opaque {
            frontmatter_bytes,
            mdx: opaque.mdx.iter().map(|span| translate(*span)).collect(),
            html: opaque.html.iter().map(|span| translate(*span)).collect(),
        },
        governed: governed
            .into_iter()
            .map(|definition| GovernedDefinition {
                span: translate(definition.span),
                ..definition
            })
            .collect(),
        headings: sweep
            .headings
            .into_iter()
            .map(|heading| Heading {
                span: translate(heading.span),
                ..heading
            })
            .collect(),
        html_anchors,
        declared_anchors: sweep.declared,
    })
}

struct Sweep<'a> {
    suffix: &'a str,
    definitions: Vec<Definition>,
    orphans: OrphanDefinitions,
    root_span: (usize, usize),
    occurrences: Vec<Occurrence>,
    headings: Vec<Heading>,
    declared: Vec<String>,
    mdx: Vec<(usize, usize)>,
    html: Vec<(usize, usize)>,
}

impl Sweep<'_> {
    /// One node of the pre-order walk. Returns whether to descend: an MDX
    /// construct's outer span makes all its children opaque, so nothing inside
    /// one is extracted.
    fn visit(&mut self, node: &Node, path: &[usize], owners: &mut Owners) -> Result<bool, Fault> {
        let bytes = self.suffix.as_bytes();
        match node {
            Node::MdxjsEsm(_)
            | Node::MdxFlowExpression(_)
            | Node::MdxTextExpression(_)
            | Node::MdxJsxFlowElement(_)
            | Node::MdxJsxTextElement(_) => {
                self.mdx.push(span_of(node)?);
                return Ok(false);
            }
            Node::Html(_) => {
                let span = span_of(node)?;
                self.html.push(span);
                self.destinations(span, path, *owners);
            }
            Node::Heading(_) => {
                let content = text_content(node);
                let (text, attribute) = mdx_comment_attribute(node).map_or_else(
                    || split_attribute(&content, trailing_text(node)),
                    |id| {
                        let kept = content.trim_end();
                        let suffix = content.get(kept.len()..).unwrap_or_default().to_owned();
                        (kept.to_owned(), Some(HeadingAttribute { id, suffix }))
                    },
                );
                self.headings.push(Heading {
                    text,
                    attribute,
                    source: HeadingSource::Markdown,
                    span: span_of(node)?,
                });
            }
            Node::ListItem(_) => owners.list_item = Some(span_of(node)?),
            Node::TableCell(_) => owners.cell = Some(span_of(node)?),
            Node::Paragraph(_) => {
                owners.paragraph = Some(span_of(node)?);
                if let Some(id) = trailing_attribute(trailing_text(node)) {
                    self.declared.push(id);
                }
            }
            Node::Link(link) => {
                let span = span_of(node)?;
                let (construct, raw) = link_destination(bytes, self.suffix, span, link)?;
                self.push(construct, raw, link.url.clone(), span, path, *owners);
            }
            Node::Image(image) => {
                let span = span_of(node)?;
                let label_end = image_label_end(bytes, span)?;
                let token_span = inline_destination(bytes, label_end)?;
                let raw = token(self.suffix, token_span)?;
                self.push(
                    SourceConstruct::InlineImage,
                    raw,
                    image.url.clone(),
                    span,
                    path,
                    *owners,
                );
            }
            Node::LinkReference(reference) => {
                let construct = reference_link(reference.reference_kind);
                let winning = winning(&self.definitions, &reference.identifier)?;
                if !winning.reserved {
                    let (raw, url) = (winning.raw.clone(), winning.url.clone());
                    self.push(construct, raw, url, span_of(node)?, path, *owners);
                }
            }
            Node::ImageReference(reference) => {
                let construct = reference_image(reference.reference_kind);
                let winning = winning(&self.definitions, &reference.identifier)?;
                if !winning.reserved {
                    let (raw, url) = (winning.raw.clone(), winning.url.clone());
                    self.push(construct, raw, url, span_of(node)?, path, *owners);
                }
            }
            Node::Root(_)
            | Node::Blockquote(_)
            | Node::FootnoteDefinition(_)
            | Node::List(_)
            | Node::Toml(_)
            | Node::Yaml(_)
            | Node::Break(_)
            | Node::InlineCode(_)
            | Node::InlineMath(_)
            | Node::Delete(_)
            | Node::Emphasis(_)
            | Node::FootnoteReference(_)
            | Node::Strong(_)
            | Node::Text(_)
            | Node::Code(_)
            | Node::Math(_)
            | Node::Table(_)
            | Node::ThematicBreak(_)
            | Node::TableRow(_) => {}
            // A definition nobody references still maintains a destination.
            Node::Definition(_definition) => self.orphan(node, path, *owners)?,
        }
        Ok(true)
    }

    /// `href` and `src` are read out of a raw-HTML node the way any destination
    /// is, references decoded; comments and raw-text bodies stay the blind spot.
    /// Each mined tag extends the node's path with its ordinal, the way the
    /// `AsciiDoc` and RST adapters count within a block, so two equal tags in one
    /// node keep distinct addresses.
    fn destinations(&mut self, span: (usize, usize), path: &[usize], owners: Owners) {
        let Some(region) = self.suffix.as_bytes().get(span.0..span.1) else {
            return;
        };
        let mut found: Vec<(SourceConstruct, String, (usize, usize))> = Vec::new();
        walk_region(region, |at| {
            if let Some(end) = opaque_text_end(region, at) {
                return Some(end);
            }
            let Some((construct, attribute)) = destination_open_at(region, at) else {
                return foreign_tag_end(region, at);
            };
            let end = tag_close(region, at)?;
            let value = unquoted(region, at, |inner, _byte| {
                if inner >= end {
                    return Some(None);
                }
                attribute_name_at(region, inner, attribute)
                    .then(|| attribute_value(region, inner.saturating_add(attribute.len())))
                    .flatten()
                    .map(|(value, _next)| Some(value))
            });
            if let Some(Some(value)) = value {
                found.push((
                    construct,
                    value,
                    (span.0.saturating_add(at), span.0.saturating_add(end)),
                ));
            }
            Some(end)
        });
        for (within, (construct, value, tag_span)) in found.into_iter().enumerate() {
            if let Some(semantic) = decoded(&value) {
                let mut tag_path = path.to_vec();
                tag_path.push(within);
                self.push(construct, value, semantic, tag_span, &tag_path, owners);
            }
        }
    }

    fn orphan(&mut self, node: &Node, path: &[usize], owners: Owners) -> Result<(), Fault> {
        let span = span_of(node)?;
        if let Some((raw, url)) = self.orphans.get(&span) {
            let (raw, url) = (raw.clone(), url.clone());
            // A definition is a block node holding one destination, so it takes
            // the same within-node ordinal as a mined tag; a root-level one then
            // reaches the two-element path the address shape requires.
            let mut definition_path = path.to_vec();
            definition_path.push(0);
            self.push(
                SourceConstruct::LinkReferenceDefinition,
                raw,
                url,
                span,
                &definition_path,
                owners,
            );
        }
        Ok(())
    }

    fn push(
        &mut self,
        construct: SourceConstruct,
        raw_destination: String,
        semantic_destination: String,
        span: (usize, usize),
        path: &[usize],
        owners: Owners,
    ) {
        let (block_kind, block_span) = if let Some(owner) = owners.list_item {
            (BlockKind::ListItem, owner)
        } else if let Some(owner) = owners.cell {
            (BlockKind::TableCell, owner)
        } else if let Some(owner) = owners.paragraph {
            (BlockKind::Paragraph, owner)
        } else {
            (BlockKind::DocumentRoot, self.root_span)
        };
        self.occurrences.push(Occurrence {
            construct,
            raw_destination,
            semantic_destination,
            span,
            node_path: path.to_vec(),
            block_kind,
            block_span,
            fragment_span: None,
            path_span: None,
        });
    }
}

const fn reference_link(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::Full => SourceConstruct::FullReferenceLink,
        ReferenceKind::Collapsed => SourceConstruct::CollapsedReferenceLink,
        ReferenceKind::Shortcut => SourceConstruct::ShortcutReferenceLink,
    }
}

const fn reference_image(kind: ReferenceKind) -> SourceConstruct {
    match kind {
        ReferenceKind::Full => SourceConstruct::FullReferenceImage,
        ReferenceKind::Collapsed => SourceConstruct::CollapsedReferenceImage,
        ReferenceKind::Shortcut => SourceConstruct::ShortcutReferenceImage,
    }
}

/// Classifies a parsed link by its first source byte: `[` opens an inline
/// link, `<` an angle autolink, and anything else is a GFM extended autolink
/// whose final match is the node's own span. All autolink forms share one
/// construct; span and token distinguish them.
fn link_destination(
    bytes: &[u8],
    suffix: &str,
    span: (usize, usize),
    link: &markdown::mdast::Link,
) -> Result<(SourceConstruct, String), Fault> {
    let first = bytes.get(span.0).copied().ok_or(Fault::InvalidSourceSpan)?;
    match first {
        b'[' => {
            let children_end = link
                .children
                .last()
                .map_or(Ok(span.0.saturating_add(1)), |child| {
                    span_of(child).map(|child_span| child_span.1)
                })?;
            let token_span = inline_destination(bytes, children_end)?;
            Ok((SourceConstruct::InlineLink, token(suffix, token_span)?))
        }
        b'<' => {
            if span.1 <= span.0.saturating_add(2) {
                return Err(Fault::InvalidSourceSpan);
            }
            let inside = (span.0.saturating_add(1), span.1.saturating_sub(1));
            Ok((SourceConstruct::Autolink, token(suffix, inside)?))
        }
        _ => Ok((SourceConstruct::Autolink, token(suffix, span)?)),
    }
}

/// Walks past `](`, any separating whitespace, and returns the destination
/// token: the inside of an angle form without its delimiters, or the bare run
/// under `CommonMark` escape and balanced-parenthesis rules.
fn inline_destination(bytes: &[u8], children_end: usize) -> Result<(usize, usize), Fault> {
    if bytes.get(children_end) != Some(&b']') {
        return Err(Fault::InvalidSourceSpan);
    }
    let after = children_end.saturating_add(1);
    if bytes.get(after) != Some(&b'(') {
        return Err(Fault::InvalidSourceSpan);
    }
    let at = skip_whitespace(bytes, after.saturating_add(1));
    destination_token(bytes, at)
}

fn destination_token(bytes: &[u8], at: usize) -> Result<(usize, usize), Fault> {
    if bytes.get(at) == Some(&b'<') {
        let mut cursor = at.saturating_add(1);
        while let Some(&byte) = bytes.get(cursor) {
            match byte {
                b'\\' => cursor = cursor.saturating_add(2),
                b'>' => return Ok((at.saturating_add(1), cursor)),
                _ => cursor = cursor.saturating_add(1),
            }
        }
        Err(Fault::InvalidSourceSpan)
    } else {
        let mut cursor = at;
        let mut depth = 0_usize;
        while let Some(&byte) = bytes.get(cursor) {
            match byte {
                b'\\' => cursor = cursor.saturating_add(2),
                b'(' => {
                    depth = depth.saturating_add(1);
                    cursor = cursor.saturating_add(1);
                }
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                    cursor = cursor.saturating_add(1);
                }
                b' ' | b'\t' | b'\r' | b'\n' => break,
                _ => cursor = cursor.saturating_add(1),
            }
        }
        Ok((at, cursor.min(bytes.len())))
    }
}

/// An image's label is flattened to a string in the tree, so its end is
/// recovered by scanning the source: brackets nest (an image label, unlike a
/// link label, may contain links), backslash escapes hide a bracket, and a
/// code span protects everything inside it.
fn image_label_end(bytes: &[u8], span: (usize, usize)) -> Result<usize, Fault> {
    let mut at = span.0.saturating_add(2);
    let mut depth = 1_usize;
    while at < span.1 {
        let Some(&byte) = bytes.get(at) else {
            break;
        };
        match byte {
            b'\\' => at = at.saturating_add(2),
            b'`' => at = skip_code_span(bytes, at, span.1),
            b'[' => {
                depth = depth.saturating_add(1);
                at = at.saturating_add(1);
            }
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(at);
                }
                at = at.saturating_add(1);
            }
            _ => at = at.saturating_add(1),
        }
    }
    Err(Fault::ParserError)
}

/// A code span closes only on a backtick run of exactly the opening length;
/// unmatched backticks are literal.
fn skip_code_span(bytes: &[u8], at: usize, limit: usize) -> usize {
    let open = run_length(bytes, at, limit);
    let mut cursor = at.saturating_add(open);
    while cursor < limit {
        if bytes.get(cursor) == Some(&b'`') {
            let run = run_length(bytes, cursor, limit);
            if run == open {
                return cursor.saturating_add(run);
            }
            cursor = cursor.saturating_add(run);
        } else {
            cursor = cursor.saturating_add(1);
        }
    }
    at.saturating_add(open)
}

fn run_length(bytes: &[u8], at: usize, limit: usize) -> usize {
    let mut cursor = at;
    while cursor < limit && bytes.get(cursor) == Some(&b'`') {
        cursor = cursor.saturating_add(1);
    }
    cursor.saturating_sub(at)
}

/// Collects reference definitions in document order; the first with a matching
/// normalized identifier wins.
type OrphanDefinitions = BTreeMap<(usize, usize), (String, String)>;
type ResolvedDefinitions = (Vec<Definition>, Vec<GovernedDefinition>, OrphanDefinitions);

fn definitions(tree: &Node, suffix: &str) -> Result<ResolvedDefinitions, Fault> {
    let mut out = Vec::new();
    let mut governed = Vec::new();
    let mut used = BTreeSet::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        if let Node::LinkReference(reference) = node {
            used.insert(reference.identifier.clone());
        }
        if let Node::ImageReference(reference) = node {
            used.insert(reference.identifier.clone());
        }
        if let Node::Definition(definition) = node {
            let span = span_of(node)?;
            let label = definition
                .label
                .as_deref()
                .unwrap_or(definition.identifier.as_str());
            let (raw, angled) = definition_destination(suffix, span)?;
            let reserved = label.starts_with(RESERVED_LABEL_PREFIX);
            if reserved {
                governed.push(GovernedDefinition {
                    span,
                    url: definition.url.clone(),
                    title: definition.title.clone(),
                    label: label.to_owned(),
                    angled,
                });
            }
            out.push((
                span,
                Definition {
                    identifier: definition.identifier.clone(),
                    url: definition.url.clone(),
                    raw,
                    reserved,
                },
            ));
        }
        if let Some(children) = node.children() {
            stack.extend(children.iter().rev());
        }
    }
    out.sort_by_key(|(span, _)| *span);
    governed.sort_by_key(|definition| definition.span);
    let orphans = out
        .iter()
        .filter(|(_, definition)| !definition.reserved && !used.contains(&definition.identifier))
        .map(|(span, definition)| (*span, (definition.raw.clone(), definition.url.clone())))
        .collect();
    Ok((
        out.into_iter().map(|(_, definition)| definition).collect(),
        governed,
        orphans,
    ))
}

fn winning<'a>(definitions: &'a [Definition], identifier: &str) -> Result<&'a Definition, Fault> {
    definitions
        .iter()
        .find(|definition| definition.identifier == identifier)
        .ok_or(Fault::ParserError)
}

/// The raw destination token and whether it was written in angle brackets.
fn definition_destination(suffix: &str, span: (usize, usize)) -> Result<(String, bool), Fault> {
    let bytes = suffix.as_bytes();
    let mut label = span.0;
    while matches!(bytes.get(label), Some(&(b' ' | b'\t'))) {
        label = label.saturating_add(1);
    }
    if bytes.get(label) != Some(&b'[') {
        return Err(Fault::InvalidSourceSpan);
    }
    let mut at = label.saturating_add(1);
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'\\' => at = at.saturating_add(2),
            b']' => break,
            _ => at = at.saturating_add(1),
        }
    }
    if bytes.get(at) != Some(&b']') {
        return Err(Fault::InvalidSourceSpan);
    }
    if bytes.get(at.saturating_add(1)) != Some(&b':') {
        return Err(Fault::InvalidSourceSpan);
    }
    let start = skip_whitespace(bytes, at.saturating_add(2));
    let angled = bytes.get(start) == Some(&b'<');
    let token_span = destination_token(bytes, start)?;
    Ok((token(suffix, token_span)?, angled))
}

fn token(suffix: &str, span: (usize, usize)) -> Result<String, Fault> {
    suffix
        .get(span.0..span.1)
        .map(str::to_owned)
        .ok_or(Fault::InvalidSourceSpan)
}

/// Skips the whitespace between a construct's syntax and its destination. A
/// destination may sit on the next line, and inside a block quote that line
/// resumes with the container's own `>` markers, which are line prefix, not
/// destination bytes.
fn skip_whitespace(bytes: &[u8], at: usize) -> usize {
    let mut cursor = at;
    while let Some(&byte) = bytes.get(cursor) {
        match byte {
            b' ' | b'\t' | b'\r' => cursor = cursor.saturating_add(1),
            b'\n' => {
                cursor = cursor.saturating_add(1);
                loop {
                    let mut probe = cursor;
                    let mut indent = 0_usize;
                    while indent < 3 && bytes.get(probe) == Some(&b' ') {
                        probe = probe.saturating_add(1);
                        indent = indent.saturating_add(1);
                    }
                    if bytes.get(probe) == Some(&b'>') {
                        cursor = probe.saturating_add(1);
                    } else {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    cursor
}

fn span_of(node: &Node) -> Result<(usize, usize), Fault> {
    let position = node.position().ok_or(Fault::InvalidSourceSpan)?;
    let span = (position.start.offset, position.end.offset);
    if span.0 > span.1 {
        return Err(Fault::InvalidSourceSpan);
    }
    Ok(span)
}

/// Sorts by `(start, end)`, discards any span contained in another, and unions
/// overlapping or exactly adjacent spans into maximal disjoint intervals.
fn union(mut spans: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if let Some(last) = out.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        out.push((start, end));
    }
    out
}

/// The text a renderer slugs a heading by: text with code and math verbatim,
/// and nothing from an image, raw HTML, MDX, or a footnote call. An image
/// carries its alt text in an attribute, which is not element text, so no
/// renderer reads it here.
fn text_content(node: &Node) -> String {
    let mut out = String::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current {
            Node::Text(text) => out.push_str(&text.value),
            Node::InlineCode(code) => out.push_str(&code.value),
            Node::InlineMath(math) => out.push_str(&math.value),
            Node::Code(code) => out.push_str(&code.value),
            Node::Math(math) => out.push_str(&math.value),
            Node::Break(_)
            | Node::Definition(_)
            | Node::FootnoteReference(_)
            | Node::Html(_)
            | Node::Image(_)
            | Node::ImageReference(_)
            | Node::MdxFlowExpression(_)
            | Node::MdxJsxFlowElement(_)
            | Node::MdxJsxTextElement(_)
            | Node::MdxTextExpression(_)
            | Node::MdxjsEsm(_)
            | Node::ThematicBreak(_)
            | Node::Toml(_)
            | Node::Yaml(_) => {}
            Node::Blockquote(_)
            | Node::Delete(_)
            | Node::Emphasis(_)
            | Node::FootnoteDefinition(_)
            | Node::Heading(_)
            | Node::Link(_)
            | Node::LinkReference(_)
            | Node::List(_)
            | Node::ListItem(_)
            | Node::Paragraph(_)
            | Node::Root(_)
            | Node::Strong(_)
            | Node::Table(_)
            | Node::TableCell(_)
            | Node::TableRow(_) => {
                if let Some(children) = current.children() {
                    stack.extend(children.iter().rev());
                }
            }
        }
    }
    out
}

/// Docusaurus writes a heading's identity as an MDX comment, because the
/// attribute spelling is an expression there. The comment is the heading's last
/// child and the identity is taken as written, case and all.
fn mdx_comment_attribute(node: &Node) -> Option<String> {
    let Node::MdxTextExpression(expression) = node.children()?.last()? else {
        return None;
    };
    let inner = expression
        .value
        .strip_prefix("/*")?
        .strip_suffix("*/")?
        .trim()
        .strip_prefix('#')?;
    (!inner.is_empty() && !inner.contains(char::is_whitespace)).then(|| inner.to_owned())
}

/// The literal text a block ends with, which is where `attr_list` looks for an
/// attribute block. Anything else last, inline code above all, means the block
/// carries none however its flattened content reads.
fn trailing_text(node: &Node) -> Option<&str> {
    let last = node.children()?.last()?;
    if let Node::Text(text) = last {
        Some(text.value.as_str())
    } else {
        None
    }
}

/// Splits a trailing attribute block from the heading text. The block is
/// recognized in the trailing literal text and removed from the flattened
/// content, so the text a renderer that ignores the syntax reads is `text`
/// followed by `suffix`.
fn split_attribute(content: &str, tail: Option<&str>) -> (String, Option<HeadingAttribute>) {
    let whole = || (content.to_owned(), None);
    let Some(text) = tail else {
        return whole();
    };
    let trimmed = text.trim_end();
    let Some(open) = trimmed.rfind('{') else {
        return whole();
    };
    let Some(inner) = trimmed
        .strip_suffix('}')
        .and_then(|body| body.get(open.saturating_add(1)..))
    else {
        return whole();
    };
    let Some(id) = attribute_id(inner) else {
        return whole();
    };
    let Some(head) = trimmed.get(..open).map(str::trim_end) else {
        return whole();
    };
    let Some(removed) = text.get(head.len()..) else {
        return whole();
    };
    (
        head.to_owned(),
        Some(HeadingAttribute {
            id,
            suffix: removed.to_owned(),
        }),
    )
}

/// The identity a block's own final line declares. `attr_list` applies a block
/// that stands alone on the last line to the block itself, and applies nothing
/// to one that merely trails other text, which is what the extension does.
fn trailing_attribute(text: Option<&str>) -> Option<String> {
    let last = text?.trim_end().lines().next_back()?.trim();
    let inner = last.strip_prefix('{')?.strip_suffix('}')?;
    attribute_id(inner)
}

/// The identity an `attr_list` block declares, in any of the spellings the
/// extension accepts: `#id`, `id=value`, and `id="value"`, alone or among
/// classes, with or without kramdown's leading colon. The last one wins, as it
/// does in the extension.
fn attribute_id(inner: &str) -> Option<String> {
    let inner = inner.strip_prefix(':').unwrap_or(inner).trim();
    if inner.contains(['{', '}']) {
        return None;
    }
    let mut found: Option<String> = None;
    for item in inner.split_whitespace() {
        let value = if let Some(bare) = item.strip_prefix('#') {
            bare
        } else if let Some(raw) = item.strip_prefix("id=") {
            raw.trim_matches(['"', '\''])
        } else if item.starts_with('.') || item.contains('=') {
            continue;
        } else {
            return None;
        };
        if value.is_empty() {
            return None;
        }
        found = Some(value.to_owned());
    }
    found
}

/// Every `id` and `name` attribute value inside the raw-HTML regions, in
/// document order. Accepting more than a browser would can only leave an
/// anchor unreported, never report a live one as missing.
fn html_anchors(suffix: &str, regions: &[(usize, usize)]) -> Vec<String> {
    let mut out = Vec::new();
    for (_start, region) in slices(suffix, regions) {
        walk_region(region, |at| {
            let name = ["id", "name"]
                .into_iter()
                .find(|name| attribute_name_at(region, at, name.as_bytes()))?;
            let after = at.saturating_add(name.len());
            let Some((value, next)) = attribute_value(region, after) else {
                return Some(after);
            };
            out.push(value);
            Some(next)
        });
    }
    out
}

/// Every `h1` through `h6` element written inside the raw-HTML regions, with
/// the text content its renderer would read. An element whose closing tag is
/// missing from its own region is left out.
fn html_headings(suffix: &str, regions: &[(usize, usize)]) -> Vec<Heading> {
    let mut out = Vec::new();
    for (start, region) in slices(suffix, regions) {
        // One failed search proves the level has no closer left, so a region of
        // openers costs six scans rather than one per opener.
        let mut unclosed = [false; 6];
        walk_region(region, |at| {
            let level = heading_open_at(region, at)?;
            let depth = usize::from(level.saturating_sub(b'1'));
            let Some(open_end) = tag_end(region, at) else {
                return Some(region.len());
            };
            if unclosed.get(depth) == Some(&true) {
                return Some(open_end);
            }
            let Some(close) = closing_tag(region, open_end, level) else {
                if let Some(flag) = unclosed.get_mut(depth) {
                    *flag = true;
                }
                return Some(open_end);
            };
            if let Some(inner) = region
                .get(open_end..close)
                .and_then(|raw| core::str::from_utf8(raw).ok())
            {
                out.push(Heading {
                    text: strip_markup(inner),
                    attribute: None,
                    source: HeadingSource::RawHtml,
                    span: (
                        start.saturating_add(at),
                        start.saturating_add(tag_end(region, close).unwrap_or(region.len())),
                    ),
                });
            }
            Some(close)
        });
    }
    out
}

/// Every position in one region, advancing by whatever the step recognized or
/// by one byte when it recognized nothing.
fn walk_region(region: &[u8], mut step: impl FnMut(usize) -> Option<usize>) {
    let mut at = 0_usize;
    while at < region.len() {
        at = step(at).unwrap_or_else(|| at.saturating_add(1));
    }
}

fn destination_open_at(region: &[u8], at: usize) -> Option<(SourceConstruct, &'static [u8])> {
    if region.get(at) != Some(&b'<') {
        return None;
    }
    for (name, construct, attribute) in [
        (
            b"a".as_slice(),
            SourceConstruct::HtmlAnchor,
            b"href".as_slice(),
        ),
        (
            b"img".as_slice(),
            SourceConstruct::HtmlImage,
            b"src".as_slice(),
        ),
    ] {
        let after = region.get(at.saturating_add(1).saturating_add(name.len()));
        let opens = region
            .get(at.saturating_add(1)..at.saturating_add(1).saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && after
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/');
        if opens {
            return Some((construct, attribute));
        }
    }
    None
}

const RAW_TEXT_ELEMENTS: [&[u8]; 4] = [b"script", b"style", b"textarea", b"title"];

/// A comment or raw-text element body: no renderer follows a tag spelled
/// inside one, so the miner steps over the whole span.
fn opaque_text_end(region: &[u8], at: usize) -> Option<usize> {
    if region.get(at) != Some(&b'<') {
        return None;
    }
    if region.get(at..at.saturating_add(4)) == Some(b"<!--") {
        return Some(comment_end(region, at));
    }
    let name = RAW_TEXT_ELEMENTS.into_iter().find(|name| {
        let start = at.saturating_add(1);
        region
            .get(start..start.saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && region
                .get(start.saturating_add(name.len()))
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/')
    })?;
    Some(raw_text_end(region, at, name))
}

/// Any other tag is consumed whole, so a raw-text opener spelled inside its
/// quoted attribute values is never mistaken for markup.
fn foreign_tag_end(region: &[u8], at: usize) -> Option<usize> {
    let named = matches!(region.get(at), Some(&b'<'))
        && region
            .get(at.saturating_add(1))
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'/');
    if named { tag_close(region, at) } else { None }
}

fn comment_end(region: &[u8], from: usize) -> usize {
    let start = from.saturating_add(4);
    region
        .windows(3)
        .skip(start)
        .position(|window| window == b"-->")
        .map_or(region.len(), |offset| {
            start.saturating_add(offset).saturating_add(3)
        })
}

fn raw_text_end(region: &[u8], from: usize, name: &[u8]) -> usize {
    let mut at = from.saturating_add(1);
    while at < region.len() {
        let after = at.saturating_add(2).saturating_add(name.len());
        let closes = region.get(at..at.saturating_add(2)) == Some(b"</")
            && region
                .get(at.saturating_add(2)..after)
                .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
            && region
                .get(after)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>');
        if closes {
            return tag_end(region, at).unwrap_or(region.len());
        }
        at = at.saturating_add(1);
    }
    region.len()
}

/// Every position outside quoted attribute values, visited in order until the
/// visitor answers; quoted spans are stepped over whole.
fn unquoted<T>(
    region: &[u8],
    from: usize,
    mut visit: impl FnMut(usize, u8) -> Option<T>,
) -> Option<T> {
    let mut quote: Option<u8> = None;
    let mut at = from;
    while let Some(byte) = region.get(at).copied() {
        if let Some(mark) = quote {
            if byte == mark {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if let Some(result) = visit(at, byte) {
            return Some(result);
        }
        at = at.saturating_add(1);
    }
    None
}

/// The `>` that ends the tag; an unclosed one yields nothing rather than a
/// truncated span.
fn tag_close(region: &[u8], from: usize) -> Option<usize> {
    unquoted(region, from, |at, byte| {
        (byte == b'>').then(|| at.saturating_add(1))
    })
}

fn heading_open_at(region: &[u8], at: usize) -> Option<u8> {
    if region.get(at) != Some(&b'<')
        || !matches!(region.get(at.saturating_add(1)), Some(b'h' | b'H'))
    {
        return None;
    }
    let level = *region.get(at.saturating_add(2))?;
    let after = *region.get(at.saturating_add(3))?;
    ((b'1'..=b'6').contains(&level)
        && (after.is_ascii_whitespace() || after == b'>' || after == b'/'))
        .then_some(level)
}

fn slices<'a>(
    suffix: &'a str,
    regions: &'a [(usize, usize)],
) -> impl Iterator<Item = (usize, &'a [u8])> {
    regions
        .iter()
        .filter_map(|(start, end)| Some((*start, suffix.as_bytes().get(*start..*end)?)))
}

fn scan(region: &[u8], from: usize, hit: impl Fn(usize) -> bool) -> Option<usize> {
    (from..region.len()).find(|at| hit(*at))
}

fn tag_end(region: &[u8], from: usize) -> Option<usize> {
    scan(region, from, |at| region.get(at) == Some(&b'>')).map(|at| at.saturating_add(1))
}

fn closing_tag(region: &[u8], from: usize, level: u8) -> Option<usize> {
    scan(region, from, |at| {
        region.get(at) == Some(&b'<')
            && region.get(at.saturating_add(1)) == Some(&b'/')
            && matches!(region.get(at.saturating_add(2)), Some(b'h' | b'H'))
            && region.get(at.saturating_add(3)) == Some(&level)
    })
}

/// The text content a browser reads from one element's markup: nested tags and
/// comments contribute nothing, character references decode, and every other
/// byte survives exactly, including the whitespace a wrapped element carries.
fn strip_markup(inner: &str) -> String {
    let mut out = String::with_capacity(inner.len());
    let mut rest = inner;
    while let Some(at) = rest.find(['<', '&']) {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        rest = if let Some(comment) = tail.strip_prefix("<!--") {
            comment
                .find("-->")
                .and_then(|end| comment.get(end.saturating_add(3)..))
                .unwrap_or_default()
        } else if tail.starts_with('<') {
            tail.find('>')
                .and_then(|end| tail.get(end.saturating_add(1)..))
                .unwrap_or_default()
        } else if let Some((decoded, next)) = reference(tail) {
            out.push(decoded);
            next
        } else {
            out.push('&');
            tail.get(1..).unwrap_or_default()
        };
    }
    out.push_str(rest);
    out
}

/// A destination's character references decoded, the format's own semantic
/// reading. A bare ampersand that forms no reference stays itself; a
/// reference-shaped run the table cannot decode yields nothing, so the
/// destination stays a blind spot rather than a half-decoded miss.
fn decoded(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        if let Some((symbol, next)) = reference(tail) {
            out.push(symbol);
            rest = next;
        } else if reference_shaped(tail) {
            return None;
        } else {
            out.push('&');
            rest = tail.get(1..).unwrap_or_default();
        }
    }
    out.push_str(rest);
    Some(out)
}

fn reference_shaped(tail: &str) -> bool {
    const LONGEST: usize = 32;
    tail.find(';')
        .filter(|end| *end <= LONGEST)
        .and_then(|end| tail.get(1..end))
        .is_some_and(|body| {
            !body.is_empty()
                && body
                    .chars()
                    .all(|symbol| symbol.is_ascii_alphanumeric() || symbol == '#')
        })
}

/// The named references HTML predefines, plus numeric ones. A run longer than
/// any of those spellings is text, not a reference.
fn reference(tail: &str) -> Option<(char, &str)> {
    const LONGEST: usize = 32;
    let end = tail.find(';').filter(|end| *end <= LONGEST)?;
    let body = tail.get(1..end)?;
    let next = tail.get(end.saturating_add(1)..)?;
    let decoded = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let digits = body.strip_prefix('#')?;
            let point = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(point)?
        }
    };
    Some((decoded, next))
}

fn attribute_name_at(region: &[u8], at: usize, name: &[u8]) -> bool {
    let before = at
        .checked_sub(1)
        .and_then(|index| region.get(index))
        .is_some_and(u8::is_ascii_whitespace);
    let after = region
        .get(at.saturating_add(name.len()))
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'=');
    before
        && after
        && region
            .get(at..at.saturating_add(name.len()))
            .is_some_and(|slice| slice.eq_ignore_ascii_case(name))
}

fn attribute_value(region: &[u8], from: usize) -> Option<(String, usize)> {
    let mut at = from;
    while region.get(at).is_some_and(u8::is_ascii_whitespace) {
        at = at.saturating_add(1);
    }
    if region.get(at) != Some(&b'=') {
        return None;
    }
    at = at.saturating_add(1);
    while region.get(at).is_some_and(u8::is_ascii_whitespace) {
        at = at.saturating_add(1);
    }
    let quote = match region.get(at).copied() {
        Some(mark @ (b'"' | b'\'')) => Some(mark),
        Some(_) | None => None,
    };
    let start = if quote.is_some() {
        at.saturating_add(1)
    } else {
        at
    };
    let mut end = start;
    while let Some(byte) = region.get(end) {
        let closes = quote.map_or_else(
            || byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/',
            |mark| *byte == mark,
        );
        if closes {
            break;
        }
        end = end.saturating_add(1);
    }
    let value = region
        .get(start..end)
        .and_then(|raw| core::str::from_utf8(raw).ok())?;
    let next = if quote.is_some() {
        end.saturating_add(1)
    } else {
        end
    };
    (!value.is_empty()).then(|| (value.to_owned(), next))
}

/// The closed source contract on every published span: inside the document,
/// not reversed, never splitting a CRLF pair, the opaque partition disjoint,
/// and every retained opaque region nonempty.
fn validate(
    occurrences: &[Occurrence],
    headings: &[Heading],
    opaque: &Opaque,
    offset: usize,
    suffix_len: usize,
    raw: &[u8],
) -> Result<(), Fault> {
    let endpoint = |at: usize| -> bool {
        let translated = at.saturating_add(offset);
        !(translated > 0
            && raw.get(translated.wrapping_sub(1)) == Some(&b'\r')
            && raw.get(translated) == Some(&b'\n'))
    };
    let bounded = |span: (usize, usize)| -> bool {
        span.0 <= span.1 && span.1 <= suffix_len && endpoint(span.0) && endpoint(span.1)
    };
    for entry in occurrences {
        if !bounded(entry.span) || !bounded(entry.block_span) || entry.span.0 == entry.span.1 {
            return Err(Fault::InvalidSourceSpan);
        }
    }
    for heading in headings {
        if !bounded(heading.span) || heading.span.0 == heading.span.1 {
            return Err(Fault::InvalidSourceSpan);
        }
    }
    let mut regions: Vec<(usize, usize)> = Vec::new();
    regions.extend(opaque.mdx.iter().copied());
    regions.extend(opaque.html.iter().copied());
    regions.sort_unstable();
    let mut previous_end = 0_usize;
    for (index, region) in regions.iter().enumerate() {
        if !bounded(*region) || region.0 == region.1 {
            return Err(Fault::InvalidSourceSpan);
        }
        if index > 0 && region.0 < previous_end {
            return Err(Fault::InvalidSourceSpan);
        }
        previous_end = region.1;
    }
    Ok(())
}

type SpanCore = fn(&[u8], (usize, usize), &str) -> Option<(usize, usize)>;

/// One construct gate in front of both wire span cores: an autolink is a URL
/// or email address, so its hash can sit in a local part and its text never
/// names a repository path.
fn gated_span(
    core: SpanCore,
    source: &[u8],
    span: (usize, usize),
    raw_destination: &str,
    construct: SourceConstruct,
) -> Option<(usize, usize)> {
    if construct == SourceConstruct::Autolink {
        return None;
    }
    core(source, span, raw_destination)
}
