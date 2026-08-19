use amiss_wire::extraction::{Fault, Heading, HeadingAttribute, HeadingSource};
use markdown::mdast::Node;

use super::span::span_of;

pub(super) fn markdown_heading(node: &Node) -> Result<Heading, Fault> {
    let content = text_content(node);
    let (text, attribute) = mdx_comment_attribute(node).map_or_else(
        || split_attribute(&content, trailing_text(node)),
        |id| {
            let kept = content.trim_end();
            let suffix = content.get(kept.len()..).unwrap_or_default().to_owned();
            (kept.to_owned(), Some(HeadingAttribute { id, suffix }))
        },
    );
    Ok(Heading {
        text,
        attribute,
        source: HeadingSource::Markdown,
        span: span_of(node)?,
    })
}

/// The identity a block's own final line declares. `attr_list` applies a block
/// that stands alone on the last line to the block itself, and applies nothing
/// to one that merely trails other text, which is what the extension does.
pub(super) fn paragraph_attribute(node: &Node) -> Option<String> {
    let last = trailing_text(node)?.trim_end().lines().next_back()?.trim();
    let inner = last.strip_prefix('{')?.strip_suffix('}')?;
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
