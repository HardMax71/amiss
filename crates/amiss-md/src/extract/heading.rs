use amiss_wire::extraction::{Heading, HeadingAttribute, HeadingSource};

use crate::tree::{Kind, Node};

pub(super) fn markdown_heading(node: &Node) -> Heading {
    let content = text_content(node);
    let (text, attribute) = mdx_attribute(node).map_or_else(
        || split_attribute(&content, trailing_text(node)),
        |id| {
            let kept = content.trim_end();
            let suffix = content.get(kept.len()..).unwrap_or_default().to_owned();
            (kept.to_owned(), Some(HeadingAttribute { id, suffix }))
        },
    );
    Heading {
        text,
        attribute,
        source: HeadingSource::Markdown,
        span: node.span,
    }
}

/// The identities a block's own outer lines declare. `attr_list` applies a
/// block standing alone on the last line to the block itself, `attrs_block`
/// applies one standing alone above the block to what follows, and neither
/// reads one that merely trails other text. A block that is nothing but an
/// attribute block is its own last line, so it is read once.
pub(super) fn paragraph_attribute(node: &Node) -> Vec<String> {
    let above = leading_text(node)
        .filter(|text| text.lines().nth(1).is_some())
        .and_then(|text| attribute_line(text.lines().next()));
    let below =
        trailing_text(node).and_then(|text| attribute_line(text.trim_end().lines().next_back()));
    above.into_iter().chain(below).collect()
}

/// The terms a definition list writes, which Hugo publishes an identity for
/// under `autoDefinitionTermID`.
pub(super) fn definition_terms(node: &Node) -> Vec<Heading> {
    terms(&text_content(node))
        .into_iter()
        .map(|text| Heading {
            text: text.to_owned(),
            attribute: None,
            source: HeadingSource::DefinitionTerm,
            span: node.span,
        })
        .collect()
}

/// Each term is the line above a line opening a definition, so a second
/// definition under one term names no new term.
fn terms(content: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut above: Option<&str> = None;
    for line in content.lines() {
        let defines = line.starts_with(": ") || line.starts_with(":\t");
        if defines && let Some(term) = above.map(str::trim).filter(|term| !term.is_empty()) {
            found.push(term);
        }
        above = (!defines).then_some(line);
    }
    found
}

fn attribute_line(line: Option<&str>) -> Option<String> {
    let inner = line?.trim().strip_prefix('{')?.strip_suffix('}')?;
    attribute_id(inner)
}

/// The identity a `MyST` target declares, `(name)=` alone on its line, which
/// the renderer writes onto the block that follows it. Sphinx stores the same
/// name as a label, so a `{ref}` role looks it up there too.
pub(super) fn myst_target(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix('(')?.strip_suffix(")=")?;
    (!inner.is_empty() && !inner.contains([')', '('])).then(|| inner.to_owned())
}

/// The identity an attribute block declares for the inline construct it
/// directly follows, which is the other half of what `attr_list` reads. The
/// block opens the text, because anything between it and the construct breaks
/// the pairing, and something follows it, because a block that ends its own
/// block is the one the block rule already names.
pub(super) fn inline_attribute(text: &str) -> Option<String> {
    let (inner, rest) = text.strip_prefix('{')?.split_once('}')?;
    if rest.trim().is_empty() {
        return None;
    }
    attribute_id(inner)
}

/// The text a renderer slugs a heading by: text with code and math verbatim,
/// and nothing from an image, raw HTML, MDX, or a footnote call. An image
/// carries its alt text in an attribute, which is not element text, so no
/// renderer reads it here.
fn text_content(node: &Node) -> String {
    let mut out = String::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match &current.kind {
            Kind::Text(value) | Kind::InlineCode(value) | Kind::CodeBlock(value) => {
                out.push_str(value);
            }
            Kind::Html
            | Kind::Mdx { .. }
            | Kind::MdxElement { .. }
            | Kind::MdxEsm(_)
            | Kind::Image { .. }
            | Kind::ImageReference(_)
            | Kind::Definition(_) => {}
            Kind::Root
            | Kind::Paragraph
            | Kind::Heading
            | Kind::ListItem
            | Kind::TableCell
            | Kind::Link { .. }
            | Kind::LinkReference(_)
            | Kind::Other => stack.extend(current.children.iter().rev()),
        }
    }
    out
}

/// The identity a heading's own trailing expression declares. In MDX the
/// attribute spelling is an expression, so Docusaurus reads `{#id}` back out
/// of the escaped heading text and writes the same identity as an MDX comment
/// where that escape is off. Either spelling is the heading's last child and
/// the identity is taken as written, case and all.
fn mdx_attribute(node: &Node) -> Option<String> {
    let Kind::Mdx {
        expression: Some(expression),
    } = &node.children.last()?.kind
    else {
        return None;
    };
    let body = match expression.strip_prefix("/*") {
        Some(comment) => comment.strip_suffix("*/")?,
        None => expression.as_str(),
    };
    let inner = body.trim().strip_prefix('#')?;
    (!inner.is_empty() && !inner.contains(char::is_whitespace)).then(|| inner.to_owned())
}

/// The literal text a block ends with, which is where `attr_list` looks for an
/// attribute block. Anything else last, inline code above all, means the block
/// carries none however its flattened content reads.
fn trailing_text(node: &Node) -> Option<&str> {
    if let Kind::Text(value) = &node.children.last()?.kind {
        Some(value.as_str())
    } else {
        None
    }
}

/// The literal text a block opens with, which is where `attrs_block` writes
/// the identity of the block under it.
fn leading_text(node: &Node) -> Option<&str> {
    if let Kind::Text(value) = &node.children.first()?.kind {
        Some(value.as_str())
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
    let mut rest = inner;
    while let Some((item, tail)) = attribute_item(rest) {
        rest = tail;
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

/// One attribute and whatever follows it. A quoted value is one attribute
/// however many spaces it holds, which is what the extension's own scanner
/// reads and how `{ #with-pip data-toc-label="with pip" }` keeps its identity.
fn attribute_item(text: &str) -> Option<(&str, &str)> {
    let start = text.trim_start();
    if start.is_empty() {
        return None;
    }
    let quoted = start
        .split_once('=')
        .filter(|(key, _)| !key.contains(char::is_whitespace))
        .and_then(|(key, value)| {
            let opening = value
                .chars()
                .next()
                .filter(|ch| *ch == '"' || *ch == '\'')?;
            let closing = value.get(1..)?.find(opening)?;
            key.len().checked_add(closing)?.checked_add(3)
        });
    let width = quoted.unwrap_or_else(|| start.find(char::is_whitespace).unwrap_or(start.len()));
    Some((start.get(..width)?, start.get(width..)?))
}
