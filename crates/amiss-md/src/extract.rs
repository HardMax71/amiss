use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::Adapter;
use markdown::mdast::{Node, ReferenceKind};

use crate::accounting::{AnalyzeError, Fault, Work, parsed, plain, walk};
use crate::frontmatter;

/// The block owner of one occurrence, selected by the override order: the
/// nearest ancestor list item if any, otherwise the nearest table cell,
/// otherwise the nearest paragraph, otherwise the document root. Raw HTML can
/// never own an extracted construct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    ListItem,
    TableCell,
    DocumentRoot,
}

impl BlockKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::ListItem => "list-item",
            Self::TableCell => "table-cell",
            Self::DocumentRoot => "document-root",
        }
    }
}

/// One extracted reference. `raw_destination` is the exact source-token byte
/// slice (without syntactic angle brackets, and from the first winning
/// definition for reference forms); `semantic_destination` is the token after
/// the construct's own decoding, which is exactly what the parser publishes as
/// the node's URL. Spans are zero-based half-open byte offsets into the raw
/// document, while `node_path` is the child-index path from the
/// post-frontmatter root to the syntax node itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    pub construct: SourceConstruct,
    pub raw_destination: String,
    pub semantic_destination: String,
    pub span: (usize, usize),
    pub node_path: Vec<usize>,
    pub block_kind: BlockKind,
    pub block_span: (usize, usize),
}

/// The opaque partition of one document: the frontmatter region's byte count,
/// then MDX intervals, then raw-HTML intervals on the remaining surface. The
/// three never overlap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opaque {
    pub frontmatter_bytes: usize,
    pub mdx: Vec<(usize, usize)>,
    pub html: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extraction {
    pub occurrences: Vec<Occurrence>,
    pub opaque: Opaque,
    pub governed: Vec<GovernedDefinition>,
}

/// Everything one parse yields: the work charge, the embedded-code bytes the
/// grammar's candidate-close asks spent, and the extraction for a parsing
/// adapter. The plain adapter has no spans, addresses, or occurrences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    pub work: Work,
    pub embedded_code_bytes: u64,
    pub extraction: Option<Extraction>,
}

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

/// One reserved governed definition: its complete node span, from the opening
/// bracket through the exclusive end of the destination and title syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedDefinition {
    pub span: (usize, usize),
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
    let (resolved, governed_spans) = definitions(tree, suffix)?;
    let mut sweep = Sweep {
        suffix,
        definitions: resolved,
        root_span: span_of(tree)?,
        occurrences: Vec::new(),
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
    validate(&sweep.occurrences, &opaque, offset, suffix.len(), raw)?;

    let translate =
        |span: (usize, usize)| (span.0.saturating_add(offset), span.1.saturating_add(offset));
    Ok(Extraction {
        occurrences: sweep
            .occurrences
            .into_iter()
            .map(|entry| Occurrence {
                span: translate(entry.span),
                block_span: translate(entry.block_span),
                ..entry
            })
            .collect(),
        opaque: Opaque {
            frontmatter_bytes,
            mdx: opaque.mdx.iter().map(|span| translate(*span)).collect(),
            html: opaque.html.iter().map(|span| translate(*span)).collect(),
        },
        governed: governed_spans
            .into_iter()
            .map(|span| GovernedDefinition {
                span: translate(span),
            })
            .collect(),
    })
}

struct Sweep<'a> {
    suffix: &'a str,
    definitions: Vec<Definition>,
    root_span: (usize, usize),
    occurrences: Vec<Occurrence>,
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
            Node::Html(_) => self.html.push(span_of(node)?),
            Node::ListItem(_) => owners.list_item = Some(span_of(node)?),
            Node::TableCell(_) => owners.cell = Some(span_of(node)?),
            Node::Paragraph(_) => owners.paragraph = Some(span_of(node)?),
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
            | Node::Heading(_)
            | Node::Table(_)
            | Node::ThematicBreak(_)
            | Node::TableRow(_)
            | Node::Definition(_) => {}
        }
        Ok(true)
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
type ResolvedDefinitions = (Vec<Definition>, Vec<(usize, usize)>);

fn definitions(tree: &Node, suffix: &str) -> Result<ResolvedDefinitions, Fault> {
    let mut out = Vec::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        if let Node::Definition(definition) = node {
            let span = span_of(node)?;
            let label = definition
                .label
                .as_deref()
                .unwrap_or(definition.identifier.as_str());
            out.push((
                span,
                Definition {
                    identifier: definition.identifier.clone(),
                    url: definition.url.clone(),
                    raw: definition_destination(suffix, span)?,
                    reserved: label.starts_with(RESERVED_LABEL_PREFIX),
                },
            ));
        }
        if let Some(children) = node.children() {
            stack.extend(children.iter().rev());
        }
    }
    out.sort_by_key(|(span, _)| *span);
    let governed = out
        .iter()
        .filter(|(_, definition)| definition.reserved)
        .map(|(span, _)| *span)
        .collect();
    Ok((
        out.into_iter().map(|(_, definition)| definition).collect(),
        governed,
    ))
}

fn winning<'a>(definitions: &'a [Definition], identifier: &str) -> Result<&'a Definition, Fault> {
    definitions
        .iter()
        .find(|definition| definition.identifier == identifier)
        .ok_or(Fault::ParserError)
}

fn definition_destination(suffix: &str, span: (usize, usize)) -> Result<String, Fault> {
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
    let token_span = destination_token(bytes, start)?;
    token(suffix, token_span)
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

/// The closed source contract on every published span: inside the document,
/// not reversed, never splitting a CRLF pair, the opaque partition disjoint,
/// and every retained opaque region nonempty.
fn validate(
    occurrences: &[Occurrence],
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
