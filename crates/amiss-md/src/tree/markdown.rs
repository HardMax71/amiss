use std::iter::Peekable;
use std::vec::IntoIter;

use amiss_wire::extraction::{Fault, HeadingSource};
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{Event, LinkType, Options, Parser, RefDefs, Tag, TagEnd};

use super::{Definition, Kind, Node, Reference, ReferenceForm};

/// Builds the tree from pulldown's event stream. Definitions leave no event, so
/// they are taken from the parser's own table and placed by position; text
/// runs are one node each and carry the GFM autolink literals pulldown does
/// not recognize.
///
/// # Errors
///
/// `ParserError` when a reference names no definition or the stream is not
/// balanced.
pub(crate) fn from_markdown(suffix: &str, options: Options) -> Result<Node, Fault> {
    // pulldown ends a line only at LF or CRLF, while `CommonMark` also ends one
    // at a lone CR; both are one byte, so the swap keeps every offset.
    let lf_endings: String;
    let suffix = if suffix.contains('\r') {
        let mut bytes = suffix.as_bytes().to_vec();
        for index in 0..bytes.len() {
            let lone = bytes.get(index) == Some(&b'\r')
                && bytes.get(index.saturating_add(1)) != Some(&b'\n');
            if lone && let Some(byte) = bytes.get_mut(index) {
                *byte = b'\n';
            }
        }
        lf_endings = String::from_utf8(bytes).map_err(|_invalid| Fault::ParserError)?;
        lf_endings.as_str()
    } else {
        suffix
    };
    let mut events = Parser::new_ext(suffix, options).into_offset_iter();
    let winners = events.reference_definitions().clone();
    let definitions = definitions(suffix, options, &winners)?;
    let mut builder = Builder {
        suffix,
        definitions: definitions.into_iter().peekable(),
        frames: vec![Frame {
            node: Node::leaf(Kind::Root, (0, suffix.len())),
            seals: false,
        }],
        run: Vec::new(),
        sealed: 0,
    };
    while let Some((event, range)) = events.next() {
        builder.event(
            event,
            (range.start, range.end),
            events.reference_definitions(),
        )?;
    }
    builder.finish()
}

/// How many times the document is parsed again for definitions `CommonMark`
/// shadows; each pass surfaces the next copy of every label.
const DEFINITION_RESCANS: usize = 32;

/// Every definition the document writes, in document order: the winners the
/// parse keeps, and the shadowed copies it drops without an event. A shadowed
/// copy surfaces by blanking the definitions already found, line endings
/// kept so every offset holds, and parsing again. A document that still hides
/// definitions after `DEFINITION_RESCANS` passes is a parser failure, named
/// in the report, rather than an unbounded rescan.
fn definitions(suffix: &str, options: Options, winners: &RefDefs<'_>) -> Result<Vec<Node>, Fault> {
    let mut found = definition_nodes(winners, winners);
    if found.is_empty() {
        return Ok(found);
    }
    let mut text = suffix.as_bytes().to_vec();
    let mut latest: Vec<(usize, usize)> = found.iter().map(|node| node.span).collect();
    for _rescan in 0..DEFINITION_RESCANS {
        blank(&mut text, &latest);
        let again = std::str::from_utf8(&text).map_err(|_invalid| Fault::ParserError)?;
        let more = definition_nodes(
            Parser::new_ext(again, options).reference_definitions(),
            winners,
        );
        if more.is_empty() {
            found.sort_by_key(|node| node.span);
            return Ok(found);
        }
        latest = more.iter().map(|node| node.span).collect();
        found.extend(more);
    }
    Err(Fault::ParserError)
}

/// One leaf per definition in `table`, keyed by the span start of its label's
/// winner so every copy of a label meets its references at one key.
fn definition_nodes(table: &RefDefs<'_>, winners: &RefDefs<'_>) -> Vec<Node> {
    table
        .iter()
        .map(|(label, definition)| {
            let key = winners
                .get(label)
                .map_or(definition.span.start, |winner| winner.span.start);
            Node::leaf(
                Kind::Definition(Definition {
                    key,
                    label: decoded(label),
                    url: definition.dest.to_string(),
                    title: definition.title.as_ref().map(ToString::to_string),
                }),
                (definition.span.start, definition.span.end),
            )
        })
        .collect()
}

/// A label the way mdast publishes it: backslash escapes and character
/// references decoded and nothing else read. pulldown keeps that decoder to
/// itself but applies it to a definition's title, so the label is read back
/// through one, under whichever quoting the label leaves free.
fn decoded(label: &str) -> String {
    if !label.contains(['&', '\\']) {
        return label.to_owned();
    }
    let title = [('"', '"'), ('\'', '\''), ('(', ')')]
        .into_iter()
        .find(|(_, close)| !label.contains(*close))
        .and_then(|(open, close)| {
            let document = format!("[l]: <> {open}{label}{close}");
            Parser::new(&document)
                .reference_definitions()
                .get("l")?
                .title
                .as_ref()
                .map(ToString::to_string)
        });
    title.unwrap_or_else(|| label.to_owned())
}

fn blank(text: &mut [u8], spans: &[(usize, usize)]) {
    for span in spans {
        for byte in text.get_mut(span.0..span.1).unwrap_or_default() {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }
}

struct Piece {
    span: (usize, usize),
    text: String,
}

/// An open container. A sealing frame (link, image, code, raw HTML) keeps
/// autolink literals from forming in the text under it.
struct Frame {
    node: Node,
    seals: bool,
}

struct Builder<'a> {
    suffix: &'a str,
    definitions: Peekable<IntoIter<Node>>,
    frames: Vec<Frame>,
    run: Vec<Piece>,
    sealed: usize,
}

enum Target {
    Url(String),
    Reference(Reference),
}

impl Builder<'_> {
    fn event(
        &mut self,
        event: Event<'_>,
        span: (usize, usize),
        definitions: &RefDefs<'_>,
    ) -> Result<(), Fault> {
        match event {
            Event::Start(tag) => {
                self.flush_run()?;
                self.flush_definitions(span.0)?;
                let (kind, seals) = open(tag, definitions)?;
                let span = collapsed_span(self.suffix, &kind, span);
                self.sealed = self.sealed.saturating_add(usize::from(seals));
                self.frames.push(Frame {
                    node: Node::leaf(kind, span),
                    seals,
                });
            }
            Event::End(end) => {
                self.flush_run()?;
                let closing = self.top()?.node.span.1;
                self.flush_definitions(closing)?;
                let Frame { mut node, seals } = self.frames.pop().ok_or(Fault::ParserError)?;
                self.sealed = self.sealed.saturating_sub(usize::from(seals));
                close(&mut node, end, self.suffix);
                self.top()?.node.children.push(node);
            }
            Event::Text(text) => self.run.push(Piece {
                span,
                text: text.into_string(),
            }),
            Event::SoftBreak => self.run.push(Piece {
                span,
                text: self
                    .suffix
                    .get(span.0..span.1)
                    .unwrap_or_default()
                    .to_owned(),
            }),
            Event::Code(text) | Event::InlineMath(text) | Event::DisplayMath(text) => {
                self.leaf(Kind::InlineCode(text.into_string()), span)?;
            }
            Event::Html(_) | Event::InlineHtml(_) => self.leaf(Kind::Html, span)?,
            Event::FootnoteReference(label) => {
                let kind = Kind::Footnote {
                    label: label.into_string(),
                    source: HeadingSource::FootnoteReference,
                };
                self.leaf(kind, span)?;
            }
            Event::HardBreak | Event::Rule | Event::TaskListMarker(_) => {
                self.leaf(Kind::Other, span)?;
            }
        }
        Ok(())
    }

    fn top(&mut self) -> Result<&mut Frame, Fault> {
        self.frames.last_mut().ok_or(Fault::ParserError)
    }

    fn leaf(&mut self, kind: Kind, span: (usize, usize)) -> Result<(), Fault> {
        self.flush_run()?;
        self.flush_definitions(span.0)?;
        self.top()?.node.children.push(Node::leaf(kind, span));
        Ok(())
    }

    /// Every pending definition that ends at or before `before` belongs to the
    /// open frame: a closed sibling would already have taken it.
    fn flush_definitions(&mut self, before: usize) -> Result<(), Fault> {
        while self
            .definitions
            .peek()
            .is_some_and(|node| node.span.1 <= before)
        {
            if let Some(node) = self.definitions.next() {
                self.top()?.node.children.push(node);
            }
        }
        Ok(())
    }

    fn flush_run(&mut self) -> Result<(), Fault> {
        let pieces = std::mem::take(&mut self.run);
        let (Some(first), Some(last)) = (pieces.first(), pieces.last()) else {
            return Ok(());
        };
        let span = (first.span.0, last.span.1);
        let literals = if self.sealed == 0 {
            literals(self.suffix, span, &pieces)
        } else {
            Vec::new()
        };
        let top = self.top()?;
        let mut cursor = span.0;
        for (start, end, url) in literals {
            top.node.children.extend(text(&pieces, cursor, start));
            top.node.children.push(Node {
                kind: Kind::Link { url },
                span: (start, end),
                children: text(&pieces, start, end).into_iter().collect(),
            });
            cursor = end;
        }
        top.node.children.extend(text(&pieces, cursor, span.1));
        Ok(())
    }

    fn finish(mut self) -> Result<Node, Fault> {
        self.flush_run()?;
        self.flush_definitions(usize::MAX)?;
        let root = self.frames.pop().ok_or(Fault::ParserError)?;
        if !self.frames.is_empty() {
            return Err(Fault::ParserError);
        }
        Ok(root.node)
    }
}

fn open(tag: Tag<'_>, definitions: &RefDefs<'_>) -> Result<(Kind, bool), Fault> {
    Ok(match tag {
        Tag::Paragraph => (Kind::Paragraph, false),
        Tag::Heading { .. } => (Kind::Heading, false),
        Tag::Item => (Kind::ListItem, false),
        Tag::TableCell => (Kind::TableCell, false),
        Tag::HtmlBlock => (Kind::Html, true),
        Tag::CodeBlock(_) => (Kind::CodeBlock(String::new()), true),
        Tag::Link {
            link_type,
            dest_url,
            id,
            ..
        } => (
            match target(link_type, &dest_url, &id, definitions)? {
                Target::Url(url) => Kind::Link { url },
                Target::Reference(reference) => Kind::LinkReference(reference),
            },
            true,
        ),
        Tag::Image {
            link_type,
            dest_url,
            id,
            ..
        } => (
            match target(link_type, &dest_url, &id, definitions)? {
                Target::Url(url) => Kind::Image { url },
                Target::Reference(reference) => Kind::ImageReference(reference),
            },
            true,
        ),
        Tag::FootnoteDefinition(label) => (
            Kind::Footnote {
                label: label.into_string(),
                source: HeadingSource::FootnoteDefinition,
            },
            false,
        ),
        Tag::BlockQuote(_)
        | Tag::List(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Table(_)
        | Tag::TableHead
        | Tag::TableRow
        | Tag::Emphasis
        | Tag::Strong
        | Tag::Strikethrough
        | Tag::Superscript
        | Tag::Subscript
        | Tag::MetadataBlock(_) => (Kind::Other, false),
    })
}

/// pulldown ends a collapsed reference at its label; the node covers the `[]`
/// that makes it collapsed, as mdast does.
fn collapsed_span(suffix: &str, kind: &Kind, span: (usize, usize)) -> (usize, usize) {
    let collapsed = matches!(
        kind,
        Kind::LinkReference(Reference {
            form: ReferenceForm::Collapsed,
            ..
        }) | Kind::ImageReference(Reference {
            form: ReferenceForm::Collapsed,
            ..
        })
    );
    let follows = suffix.as_bytes().get(span.1..span.1.saturating_add(2)) == Some(b"[]".as_slice());
    if collapsed && follows {
        (span.0, span.1.saturating_add(2))
    } else {
        span
    }
}

/// pulldown publishes an email autolink without its scheme; the tree carries
/// the URL the grammar constructs, as mdast does.
fn target(
    link_type: LinkType,
    dest_url: &str,
    id: &str,
    definitions: &RefDefs<'_>,
) -> Result<Target, Fault> {
    let form = match link_type {
        LinkType::Inline | LinkType::Autolink | LinkType::WikiLink { .. } => {
            return Ok(Target::Url(dest_url.to_owned()));
        }
        LinkType::Email => return Ok(Target::Url(format!("mailto:{dest_url}"))),
        LinkType::Reference | LinkType::ReferenceUnknown => ReferenceForm::Full,
        LinkType::Collapsed | LinkType::CollapsedUnknown => ReferenceForm::Collapsed,
        LinkType::Shortcut | LinkType::ShortcutUnknown => ReferenceForm::Shortcut,
    };
    let key = definitions.get(id).ok_or(Fault::ParserError)?.span.start;
    Ok(Target::Reference(Reference { key, form }))
}

/// A code block becomes one leaf holding its text, raw HTML and images become
/// leaves, and a block's span ends at its last content byte.
fn close(node: &mut Node, end: TagEnd, suffix: &str) {
    match end {
        TagEnd::CodeBlock => {
            let value: String = node
                .children
                .drain(..)
                .filter_map(|child| {
                    if let Kind::Text(text) = child.kind {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect();
            node.kind = Kind::CodeBlock(without_final_line_ending(value));
        }
        TagEnd::HtmlBlock | TagEnd::Image => node.children.clear(),
        TagEnd::Paragraph
        | TagEnd::Heading(_)
        | TagEnd::BlockQuote(_)
        | TagEnd::List(_)
        | TagEnd::Item
        | TagEnd::FootnoteDefinition
        | TagEnd::DefinitionList
        | TagEnd::DefinitionListTitle
        | TagEnd::DefinitionListDefinition
        | TagEnd::Table
        | TagEnd::TableHead
        | TagEnd::TableRow
        | TagEnd::TableCell
        | TagEnd::Emphasis
        | TagEnd::Strong
        | TagEnd::Strikethrough
        | TagEnd::Superscript
        | TagEnd::Subscript
        | TagEnd::Link
        | TagEnd::MetadataBlock(_) => {}
    }
    if block(end) {
        node.span = trimmed(suffix, node.span);
    }
}

const fn block(end: TagEnd) -> bool {
    match end {
        TagEnd::Paragraph
        | TagEnd::Heading(_)
        | TagEnd::BlockQuote(_)
        | TagEnd::CodeBlock
        | TagEnd::HtmlBlock
        | TagEnd::List(_)
        | TagEnd::Item
        | TagEnd::FootnoteDefinition
        | TagEnd::DefinitionList
        | TagEnd::DefinitionListTitle
        | TagEnd::DefinitionListDefinition
        | TagEnd::Table
        | TagEnd::TableHead
        | TagEnd::TableRow
        | TagEnd::TableCell
        | TagEnd::MetadataBlock(_) => true,
        TagEnd::Emphasis
        | TagEnd::Strong
        | TagEnd::Strikethrough
        | TagEnd::Superscript
        | TagEnd::Subscript
        | TagEnd::Link
        | TagEnd::Image => false,
    }
}

fn trimmed(suffix: &str, span: (usize, usize)) -> (usize, usize) {
    let bytes = suffix.as_bytes();
    let mut end = span.1;
    while end > span.0
        && bytes
            .get(end.wrapping_sub(1))
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        end = end.saturating_sub(1);
    }
    (span.0, end)
}

fn without_final_line_ending(mut value: String) -> String {
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    value
}

/// A piece is exact when its decoded text is its source bytes; an entity or
/// a numeric reference decodes to a different length.
fn exact(piece: &Piece) -> bool {
    piece.text.len() == piece.span.1.saturating_sub(piece.span.0)
}

/// The text node covering `from..to` of a run, or none when nothing but
/// escape backslashes sits there.
fn text(pieces: &[Piece], from: usize, to: usize) -> Option<Node> {
    if from >= to {
        return None;
    }
    let mut value = String::new();
    for piece in pieces {
        let (start, end) = (piece.span.0.max(from), piece.span.1.min(to));
        if start >= end {
            continue;
        }
        if exact(piece) {
            value.push_str(
                piece
                    .text
                    .get(start.saturating_sub(piece.span.0)..end.saturating_sub(piece.span.0))
                    .unwrap_or_default(),
            );
        } else {
            value.push_str(&piece.text);
        }
    }
    (!value.is_empty()).then(|| Node::leaf(Kind::Text(value), (from, to)))
}

/// The GFM autolink literals in one text run, found over the source bytes so
/// escapes and references inside a URL stay part of it: `www.`, `http://`,
/// `https://`, email, and `mailto:` or `xmpp:` before an email. Everything
/// else linkify recognizes (bare domains, other schemes) is text here, as it
/// is on github.com.
fn literals(suffix: &str, span: (usize, usize), pieces: &[Piece]) -> Vec<(usize, usize, String)> {
    let Some(run) = suffix.get(span.0..span.1) else {
        return Vec::new();
    };
    let mut finder = LinkFinder::new();
    finder.url_must_have_scheme(false);
    finder.kinds(&[LinkKind::Url, LinkKind::Email]);
    let mut out = Vec::new();
    for link in finder.links(run) {
        let start = span.0.saturating_add(link.start());
        let end = span.0.saturating_add(link.end());
        let email = matches!(link.kind(), LinkKind::Email);
        let start = if email {
            scheme_before(suffix, span.0, start)
        } else {
            start
        };
        let Some((start, end)) = snapped(pieces, start, end) else {
            continue;
        };
        if escaped(suffix, pieces, start) {
            continue;
        }
        let end = without_entity_trailer(suffix, start, end);
        let Some(found) = suffix.get(start..end) else {
            continue;
        };
        let url = if email {
            if has_prefix(found, "mailto:") || has_prefix(found, "xmpp:") {
                found.to_owned()
            } else if found.contains('@') {
                format!("mailto:{found}")
            } else {
                continue;
            }
        } else if has_prefix(found, "http://") || has_prefix(found, "https://") {
            found.to_owned()
        } else if has_prefix(found, "www.") {
            format!("http://{found}")
        } else {
            continue;
        };
        out.push((start, end, url));
    }
    out
}

fn has_prefix(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn scheme_before(suffix: &str, floor: usize, start: usize) -> usize {
    ["mailto:", "xmpp:"]
        .into_iter()
        .find_map(|scheme| {
            let at = start.checked_sub(scheme.len())?;
            (at >= floor
                && suffix
                    .get(at..start)
                    .is_some_and(|before| before.eq_ignore_ascii_case(scheme)))
            .then_some(at)
        })
        .unwrap_or(start)
}

/// GFM leaves a trailing `&name;` out of a literal, since it reads as a
/// character reference; linkify stops before the semicolon only.
fn without_entity_trailer(suffix: &str, start: usize, end: usize) -> usize {
    if suffix.as_bytes().get(end) != Some(&b';') {
        return end;
    }
    let found = suffix.get(start..end).unwrap_or_default();
    let Some(at) = found.rfind('&') else {
        return end;
    };
    let name = found.get(at.saturating_add(1)..).unwrap_or_default();
    if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        start.saturating_add(at)
    } else {
        end
    }
}

/// The byte before a match is an escape backslash exactly when no piece covers
/// it; the escape consumed the character a literal would have started on.
fn escaped(suffix: &str, pieces: &[Piece], start: usize) -> bool {
    let Some(before) = start.checked_sub(1) else {
        return false;
    };
    suffix.as_bytes().get(before) == Some(&b'\\')
        && !pieces
            .iter()
            .any(|piece| piece.span.0 <= before && before < piece.span.1)
}

/// A boundary strictly inside a decoded piece snaps outward, which is also
/// GFM's rule for an entity-shaped trailer: it is never part of the link.
fn snapped(pieces: &[Piece], start: usize, end: usize) -> Option<(usize, usize)> {
    let inside = |at: usize| {
        pieces
            .iter()
            .find(|piece| !exact(piece) && piece.span.0 < at && at < piece.span.1)
    };
    let start = inside(start).map_or(start, |piece| piece.span.1);
    let end = inside(end).map_or(end, |piece| piece.span.0);
    (start < end).then_some((start, end))
}
