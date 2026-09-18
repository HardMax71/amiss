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
use amiss_wire::extraction::{Transclusion, TransclusionKind, TransclusionRefusal};
use amiss_wire::model::Adapter;

use crate::accounting::{parsed, plain};
use crate::frontmatter;
use crate::tree::{Kind, Node};

pub use definition::RESERVED_LABEL_PREFIX;
use definition::{CollectedDefinitions, Definitions, OrphanDefinitions, definitions};
use source::{
    image_label_end, inline_destination, link_destination, reference_image, reference_link, token,
};
use span::{gated_span, union, validate};

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
        root_span: tree.span,
        occurrences: Vec::new(),
        headings: Vec::new(),
        declared: Vec::new(),
        imports: Vec::new(),
        rendered: Vec::new(),
        snippets: Vec::new(),
        mdx: Vec::new(),
        html: Vec::new(),
    };
    sweep_tree(tree, &mut sweep)?;
    let transclusions = declared_includes(&sweep);

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
        transclusions: transclusions
            .into_iter()
            .map(|entry| Transclusion {
                span: translate(entry.span),
                ..entry
            })
            .collect(),
        occurrences: translated_occurrences(sweep.occurrences, suffix, offset),
        opaque: Opaque {
            frontmatter_bytes,
            mdx: opaque.mdx.iter().map(|span| translate(*span)).collect(),
            html: opaque.html.iter().map(|span| translate(*span)).collect(),
        },
        governed: governed
            .into_iter()
            .map(|mut definition| {
                definition.span = translate(definition.span);
                if let Some(code) = &mut definition.previous_code {
                    code.span = translate(code.span);
                }
                definition
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

/// Every occurrence moved from the post-frontmatter suffix to the raw
/// document, with the destination spans an edit may claim located under the
/// wire's own certainty rules.
fn translated_occurrences(
    occurrences: Vec<Occurrence>,
    suffix: &str,
    offset: usize,
) -> Vec<Occurrence> {
    let translate =
        |span: (usize, usize)| (span.0.saturating_add(offset), span.1.saturating_add(offset));
    occurrences
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
        .collect()
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
        let parent_depth = path.len();
        for (index, child) in node.children.iter().enumerate().rev() {
            stack.push(Frame {
                node: child,
                parent_depth,
                index: Some(index),
                owners,
            });
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
    imports: Vec<(String, String)>,
    rendered: Vec<(String, (usize, usize))>,
    snippets: Vec<Transclusion>,
    mdx: Vec<(usize, usize)>,
    html: Vec<(usize, usize)>,
}

impl Sweep<'_> {
    /// One node of the pre-order walk. Returns whether to descend: an MDX
    /// construct's outer span makes all its children opaque, so nothing inside
    /// one is extracted.
    fn visit(&mut self, node: &Node, path: &[usize], owners: &mut Owners) -> Result<bool, Fault> {
        let bytes = self.suffix.as_bytes();
        let span = node.span;
        match &node.kind {
            Kind::Mdx { .. } | Kind::MdxElement { .. } | Kind::MdxEsm(_) => {
                self.mdx.push(span);
                mdx_declarations(self, node);
                return Ok(false);
            }
            Kind::Html => {
                self.html.push(span);
                self.snippets
                    .extend(shortcode(self.suffix.get(span.0..span.1), span));
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
            Kind::Heading => self.headings.push(heading::markdown_heading(node)),
            Kind::ListItem => owners.list_item = Some(span),
            Kind::TableCell => owners.cell = Some(span),
            Kind::Paragraph => {
                owners.paragraph = Some(span);
                self.declared.extend(heading::paragraph_attribute(node));
                self.declared.extend(heading::glossary_terms(node));
                self.declared
                    .extend(heading::directive_names(self.suffix.get(span.0..span.1)));
                self.headings.extend(heading::definition_terms(node));
            }
            Kind::Link { url } => {
                let children_end = node.children.last().map(|child| child.span.1);
                let (construct, raw) = link_destination(bytes, self.suffix, span, children_end)?;
                self.push(construct, raw, url.clone(), span, path, *owners);
            }
            Kind::Image { url } => {
                let label_end = image_label_end(bytes, span)?;
                let token_span = inline_destination(bytes, label_end)?;
                let raw = token(self.suffix, token_span)?;
                self.push(
                    SourceConstruct::InlineImage,
                    raw,
                    url.clone(),
                    span,
                    path,
                    *owners,
                );
            }
            Kind::LinkReference(reference) => {
                let construct = reference_link(reference.form);
                let winning = self.definitions.get(&reference.key);
                let winning = winning.ok_or(Fault::ParserError)?;
                if !winning.reserved {
                    let (raw, url) = (winning.raw.clone(), winning.url.clone());
                    self.push(construct, raw, url, span, path, *owners);
                }
            }
            Kind::ImageReference(reference) => {
                let construct = reference_image(reference.form);
                let winning = self.definitions.get(&reference.key);
                let winning = winning.ok_or(Fault::ParserError)?;
                if !winning.reserved {
                    let (raw, url) = (winning.raw.clone(), winning.url.clone());
                    self.push(construct, raw, url, span, path, *owners);
                }
            }
            // A definition nobody references still maintains a destination.
            Kind::Definition(_) => self.orphan(node, path, *owners),
            Kind::Text(value) => {
                if owners.paragraph.is_some() {
                    for line in value.lines() {
                        self.snippets.extend(snippet(line, span));
                        self.snippets.extend(directive(line, span));
                        self.snippets.extend(content_tab(line, span));
                        self.snippets.extend(shortcode_call(line, span));
                        self.declared.extend(heading::myst_target(line));
                    }
                }
                let after_node = path.last().is_some_and(|index| *index > 0);
                self.declared
                    .extend(heading::inline_attribute(value, after_node));
            }
            Kind::InlineCode(_) => {
                if let Some((construct, raw, semantic, role_span)) = role(self.suffix, span) {
                    self.push(construct, raw, semantic, role_span, path, *owners);
                }
            }
            Kind::CodeBlock(_) => {
                self.declared
                    .extend(heading::directive_names(self.suffix.get(span.0..span.1)));
            }
            Kind::Root | Kind::Other => {}
        }
        Ok(true)
    }

    fn orphan(&mut self, node: &Node, path: &[usize], owners: Owners) {
        if let Some((raw, url)) = self.orphans.remove(&node.span) {
            // A definition is a block node holding one destination, so it takes
            // the same within-node ordinal as a mined tag; a root-level one then
            // reaches the two-element path the address shape requires.
            let mut definition_path = path.to_vec();
            definition_path.push(0);
            self.push(
                SourceConstruct::LinkReferenceDefinition,
                raw,
                url,
                node.span,
                &definition_path,
                owners,
            );
        }
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

/// What one MDX region writes down: the identity a plain element declares, the
/// modules the block imports, and the components it renders. JSX reads a
/// lowercase tag as an HTML element and anything else as a component, whose
/// rendered output this engine does not know, so the walk stops at a component
/// and reads nothing from it or under it.
fn mdx_declarations(sweep: &mut Sweep<'_>, root: &Node) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match &node.kind {
            Kind::MdxEsm(source) => default_imports(source, &mut sweep.imports),
            Kind::MdxElement { name, id } if plain_element(name.as_deref()) => {
                sweep.declared.extend(id.clone());
            }
            Kind::MdxElement { name, .. } => {
                sweep
                    .rendered
                    .extend(name.clone().map(|name| (name, node.span)));
                continue;
            }
            Kind::Root
            | Kind::Paragraph
            | Kind::Heading
            | Kind::ListItem
            | Kind::TableCell
            | Kind::Html
            | Kind::Mdx { .. }
            | Kind::Text(_)
            | Kind::InlineCode(_)
            | Kind::CodeBlock(_)
            | Kind::Link { .. }
            | Kind::Image { .. }
            | Kind::LinkReference(_)
            | Kind::ImageReference(_)
            | Kind::Definition(_)
            | Kind::Other => {}
        }
        stack.extend(node.children.iter().rev());
    }
}

/// The partials one module imports: a default import of a relative Markdown
/// document, which is the only specifier that can name a file this tree holds.
/// A package, an alias, or a stylesheet is not a document and declares no edge.
fn default_imports(source: &str, out: &mut Vec<(String, String)>) {
    out.extend(source.lines().filter_map(default_import));
}

fn default_import(line: &str) -> Option<(String, String)> {
    let (binding, rest) = line.trim().strip_prefix("import ")?.split_once(" from ")?;
    let binding = binding.trim();
    if binding.is_empty() || binding.contains(['{', '}', ',', '*']) {
        return None;
    }
    let specifier = quoted(rest.trim().trim_end_matches(';').trim())?;
    let name = specifier.to_ascii_lowercase();
    let document = (specifier.starts_with("./") || specifier.starts_with("../"))
        && (name.as_bytes().ends_with(b".md") || name.as_bytes().ends_with(b".mdx"));
    document.then(|| (binding.to_owned(), specifier.to_owned()))
}

/// The `pymdownx.snippets` inline include: the marker, then whitespace, then a
/// quoted path, alone on its line. A path carrying a section coordinate names
/// part of a file rather than the file, which this engine cannot reproduce, so
/// the edge is refused instead of claiming the whole of it.
fn snippet(line: &str, span: (usize, usize)) -> Option<Transclusion> {
    let rest = line.trim().strip_prefix("--8<--")?;
    let target = quoted(rest.strip_prefix([' ', '\t'])?.trim())?;
    if target.is_empty() {
        return None;
    }
    let kind = if target.contains(':') {
        Err(TransclusionRefusal::Options)
    } else {
        Ok(TransclusionKind::Parsed)
    };
    Some(Transclusion {
        target: target.to_owned(),
        span,
        kind,
    })
}

/// The mkdocstrings instruction: `:::`, then whitespace, then what a
/// generator renders in its place. The rendered output carries identities
/// derived from something outside the tree, so the edge names what would
/// arrive and is refused rather than followed. An admonition fence, `:::note`,
/// carries no whitespace after the marker and names nothing.
fn directive(line: &str, span: (usize, usize)) -> Option<Transclusion> {
    generated(
        line.trim().strip_prefix(":::")?.strip_prefix([' ', '\t']),
        span,
        TransclusionRefusal::DynamicTarget,
    )
}

/// A content tab, `=== "Title"`, optionally opening a set or selected. The
/// tabbed extension publishes an identity for the title under the slug
/// function a site configures, and combines it with the heading above where
/// the site asks for that, so the tab names what would arrive and is refused.
fn content_tab(line: &str, span: (usize, usize)) -> Option<Transclusion> {
    let opened = line.trim().strip_prefix("===")?;
    let title = opened.strip_prefix(['!', '+']).unwrap_or(opened);
    generated(
        quoted(title.strip_prefix([' ', '\t'])?.trim()),
        span,
        TransclusionRefusal::DynamicTarget,
    )
}

/// The shortcode a declared hook expands before anything is rendered,
/// `<!-- md:setting name -->`. What the hook writes in the comment's place
/// carries identities of its own, so the edge names the shortcode and is
/// refused rather than followed.
fn shortcode(region: Option<&str>, span: (usize, usize)) -> Option<Transclusion> {
    let comment = region?.split_once("<!--")?.1.split_once("-->")?.0;
    generated(
        comment.trim().strip_prefix("md:"),
        span,
        TransclusionRefusal::DynamicTarget,
    )
}

/// The pair of markers a Hugo shortcode call opens and closes with: the raw
/// form, and the form whose output is rendered as Markdown.
const SHORTCODE_MARKERS: [(&str, &str); 2] = [("{{%", "%}}"), ("{{<", ">}}")];

/// A shortcode call standing alone as a block, which here is a line holding
/// one call and nothing else. A template answers it, and what that template
/// writes carries headings and definition-list terms of its own, so the call
/// names the shortcode and is refused rather than followed. A call in the flow
/// of a sentence renders inside that sentence and writes neither.
fn shortcode_call(line: &str, span: (usize, usize)) -> Option<Transclusion> {
    let called = SHORTCODE_MARKERS.iter().find_map(|(open, close)| {
        let body = line.trim().strip_prefix(open)?.strip_suffix(close)?;
        (!body.contains(close)).then_some(body)
    });
    generated(called, span, TransclusionRefusal::Template)
}

/// The edge a spelling a program answers writes: it names what would arrive
/// rather than a path the tree holds, so it is refused instead of followed,
/// under the refusal that says which program answers it.
fn generated(
    target: Option<&str>,
    span: (usize, usize),
    refusal: TransclusionRefusal,
) -> Option<Transclusion> {
    let named = target.map(str::trim).filter(|name| !name.is_empty())?;
    Some(Transclusion {
        target: named.to_owned(),
        span,
        kind: Err(refusal),
    })
}

/// A `MyST` cross-reference role, `` {doc}`quickstart` ``, read back from the
/// code span the role body parses as. `doc` and `ref` are the two Sphinx
/// answers this engine holds: a docname takes the profile's own suffix, and
/// every other role names something a domain inventory outside the tree
/// answers, which is what prefixing the role name spells.
fn role(
    suffix: &str,
    span: (usize, usize),
) -> Option<(SourceConstruct, String, String, (usize, usize))> {
    let (name, start) = role_name(suffix, span.0)?;
    let body = code_body(suffix, span)?;
    let target = body
        .rsplit_once('<')
        .and_then(|(_, tail)| tail.strip_suffix('>'))
        .unwrap_or(body)
        .trim();
    if target.is_empty() || target.contains('`') {
        return None;
    }
    let (construct, semantic) = match name {
        "doc" => (SourceConstruct::RstDocRole, docname(target)),
        "ref" => (SourceConstruct::RstRefRole, target.to_owned()),
        _ => (SourceConstruct::RstRefRole, format!("{name}:{target}")),
    };
    Some((construct, target.to_owned(), semantic, (start, span.1)))
}

/// The role name written immediately before a code span, with where it opens.
/// A name is the Sphinx character class, and a word character, an escape, or a
/// second brace before it means the braces are prose rather than a role.
fn role_name(suffix: &str, at: usize) -> Option<(&str, usize)> {
    let head = suffix.get(..at)?.strip_suffix('}')?;
    let open = head.rfind('{')?;
    let name = head.get(open.saturating_add(1)..)?;
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_+:.".contains(character))
    {
        return None;
    }
    let before = head.get(..open)?.chars().next_back();
    let prefixed = before.is_some_and(|character| {
        character.is_alphanumeric() || matches!(character, '\\' | '_' | '{')
    });
    (!prefixed).then_some((name, open))
}

/// The source text a code span holds, without the backtick runs that delimit
/// it. The parser publishes the span, so the opening run fixes both ends.
fn code_body(suffix: &str, span: (usize, usize)) -> Option<&str> {
    let raw = suffix.get(span.0..span.1)?;
    let ticks = raw.len().saturating_sub(raw.trim_start_matches('`').len());
    (ticks > 0)
        .then(|| raw.get(ticks..raw.len().saturating_sub(ticks)))
        .flatten()
}

/// A docname is extensionless in Sphinx, so the profile's own suffix is
/// appended; a source-root-absolute name keeps its slash and stays a declared
/// site route, because the engine does not know the Sphinx root.
fn docname(target: &str) -> String {
    let last = target.rsplit('/').next().unwrap_or(target);
    if target.starts_with('/') || last.contains('.') {
        target.to_owned()
    } else {
        format!("{target}.md")
    }
}

/// The body of a single- or double-quoted token, both marks the same one.
fn quoted(text: &str) -> Option<&str> {
    text.strip_prefix('"')
        .and_then(|body| body.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|body| body.strip_suffix('\''))
        })
}

/// Every include one document declares, in document order, so an expansion
/// places each one's identities where its syntax sits.
fn declared_includes(sweep: &Sweep<'_>) -> Vec<Transclusion> {
    let mut out = sweep.snippets.clone();
    out.extend(sweep.rendered.iter().filter_map(|(name, span)| {
        let (_, target) = sweep.imports.iter().find(|(binding, _)| binding == name)?;
        Some(Transclusion {
            target: target.clone(),
            span: *span,
            kind: Ok(TransclusionKind::Parsed),
        })
    }));
    out.sort_by_key(|transclusion| transclusion.span);
    out
}

/// A member or namespace name is a component whatever its case, and a fragment
/// renders its children as they are.
fn plain_element(name: Option<&str>) -> bool {
    name.is_none_or(|name| {
        name.starts_with(|first: char| first.is_ascii_lowercase()) && !name.contains(['.', ':'])
    })
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
