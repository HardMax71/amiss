mod definition;
mod heading;
mod html;
mod source;
mod span;
mod tests;

use amiss_wire::controls::SourceConstruct;
pub use amiss_wire::extraction::{
    Analysis, AnalyzeError, BlockKind, Extraction, Fault, GovernedDefinition, Heading,
    HeadingAttribute, HeadingSource, Occurrence, Opaque, Work,
};
use amiss_wire::model::Adapter;
use markdown::mdast::Node;

use crate::accounting::{parsed, plain};
use crate::frontmatter;

pub use definition::RESERVED_LABEL_PREFIX;
use definition::{CollectedDefinitions, Definitions, OrphanDefinitions, definitions};
use source::{
    image_label_end, inline_destination, link_destination, reference_image, reference_link, token,
};
use span::{gated_span, span_of, union, validate};

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
    let (extraction, work) = extract_tree(&tree, suffix, offset, source, frontmatter_bytes)?;
    Ok(Analysis {
        work,
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

struct Frame<'tree> {
    node: &'tree Node,
    parent_depth: usize,
    index: Option<usize>,
    owners: Owners,
}

fn extract_tree(
    tree: &Node,
    suffix: &str,
    offset: usize,
    raw: &[u8],
    frontmatter_bytes: usize,
) -> Result<(Extraction, Work), Fault> {
    let CollectedDefinitions {
        resolved,
        governed,
        orphans,
        work,
    } = definitions(tree, suffix)?;
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
    sweep_tree(tree, &mut sweep)?;

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
    sweep
        .headings
        .extend(html::collect_regions(suffix, &opaque.html, html::headings));
    sweep.headings.sort_by_key(|heading| heading.span);
    validate(
        &sweep.occurrences,
        &sweep.headings,
        &opaque,
        offset,
        suffix.len(),
        raw,
    )?;
    let html_anchors = html::collect_regions(suffix, &opaque.html, html::anchors);

    let translate =
        |span: (usize, usize)| (span.0.saturating_add(offset), span.1.saturating_add(offset));
    let extraction = Extraction {
        transclusions: Vec::new(),
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
    };
    Ok((extraction, work))
}

fn sweep_tree(tree: &Node, sweep: &mut Sweep<'_>) -> Result<(), Fault> {
    let mut stack = vec![Frame {
        node: tree,
        parent_depth: 0,
        index: None,
        owners: Owners::default(),
    }];
    let mut path = Vec::new();
    while let Some(Frame {
        node,
        parent_depth,
        index,
        mut owners,
    }) = stack.pop()
    {
        path.truncate(parent_depth);
        path.extend(index);
        if !sweep.visit(node, &path, &mut owners)? {
            continue;
        }
        if let Some(children) = node.children() {
            let parent_depth = path.len();
            for (index, child) in children.iter().enumerate().rev() {
                stack.push(Frame {
                    node: child,
                    parent_depth,
                    index: Some(index),
                    owners,
                });
            }
        }
    }
    Ok(())
}

struct Sweep<'a> {
    suffix: &'a str,
    definitions: Definitions,
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
                for destination in html::collect_regions(self.suffix, &[span], html::destinations) {
                    let mut tag_path = path.to_vec();
                    tag_path.push(destination.within);
                    self.push(
                        destination.construct,
                        destination.raw_destination,
                        destination.semantic_destination,
                        destination.span,
                        &tag_path,
                        *owners,
                    );
                }
            }
            Node::Heading(_) => self.headings.push(heading::markdown_heading(node)?),
            Node::ListItem(_) => owners.list_item = Some(span_of(node)?),
            Node::TableCell(_) => owners.cell = Some(span_of(node)?),
            Node::Paragraph(_) => {
                owners.paragraph = Some(span_of(node)?);
                if let Some(id) = heading::paragraph_attribute(node) {
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
                let winning = self.definitions.get(&reference.identifier);
                let winning = winning.ok_or(Fault::ParserError)?;
                if !winning.reserved {
                    let (raw, url) = (winning.raw.clone(), winning.url.clone());
                    self.push(construct, raw, url, span_of(node)?, path, *owners);
                }
            }
            Node::ImageReference(reference) => {
                let construct = reference_image(reference.reference_kind);
                let winning = self.definitions.get(&reference.identifier);
                let winning = winning.ok_or(Fault::ParserError)?;
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

    fn orphan(&mut self, node: &Node, path: &[usize], owners: Owners) -> Result<(), Fault> {
        let span = span_of(node)?;
        if let Some((raw, url)) = self.orphans.remove(&span) {
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
