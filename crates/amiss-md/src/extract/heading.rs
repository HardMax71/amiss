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
        .and_then(opening_attribute)
        .and_then(attribute_id);
    let below =
        trailing_text(node).and_then(|text| attribute_line(text.trim_end().lines().next_back()));
    above.into_iter().chain(below).collect()
}

/// The attribute block a run of lines opens with, with something under it for
/// the block to name. A directive opener leaves no blank line above the block
/// it holds, so the two share a paragraph and the block `attrs_block` writes
/// is the opener's own next line.
fn opening_attribute(text: &str) -> Option<&str> {
    let mut lines = text.lines();
    let first = lines.next()?;
    let line = if directive_opener(first).is_some() {
        lines.next()?
    } else {
        first
    };
    let inner = line.trim().strip_prefix('{')?.strip_suffix('}')?;
    lines.next().is_some().then_some(inner)
}

/// The brace-wrapped tag a `MyST` directive opens with, on a fence of at least
/// three colons, backticks or tildes, and whatever the opener writes after it.
pub(super) fn directive_opener(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let fence = trimmed
        .chars()
        .next()
        .filter(|character| matches!(character, ':' | '`' | '~'))?;
    let rest = trimmed.trim_start_matches(fence);
    if trimmed.len().saturating_sub(rest.len()) < 3 {
        return None;
    }
    rest.strip_prefix('{')?.split_once('}')
}

/// The identities a directive declares. `MyST` writes the `:name:` option
/// block under the opener, and the reStructuredText inside an `eval-rst` body
/// names its own directives the same way, so every option line under one
/// opener is read. `figure-md` takes the name as its argument instead, which
/// no other directive does: the rest write a path or a title there, or the
/// domain object `domain_object` reads.
pub(super) fn directive_names(block: Option<&str>) -> Vec<String> {
    let mut lines = block.unwrap_or_default().lines();
    let Some((tag, argument)) = lines.next().and_then(directive_opener) else {
        return Vec::new();
    };
    let argument = argument.trim();
    let figure =
        (tag == "figure-md" && !argument.is_empty() && !argument.contains(char::is_whitespace))
            .then(|| argument.to_owned());
    figure
        .into_iter()
        .chain(domain_object(tag, argument))
        .chain(lines.filter_map(amiss_wire::extraction::directive_name_option))
        .collect()
}

/// The object a Sphinx domain directive describes, which the domain stores
/// under the name written here rather than under a slug of it. A `domain:type`
/// tag is what marks one, so `{py:class} widgets.Widget` publishes that name
/// and `{note}` publishes nothing. What follows the name is the domain's own
/// signature grammar, so a name is read only where the argument holds one
/// token, with a parameter list taken off it.
fn domain_object(tag: &str, argument: &str) -> Option<String> {
    let (domain, kind) = tag.split_once(':')?;
    if domain.is_empty() || kind.is_empty() || tag.contains(char::is_whitespace) {
        return None;
    }
    let name = argument.split('(').next()?.trim_end();
    (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_owned())
}

/// The terms a glossary declares, which Sphinx keeps under the term itself
/// rather than under a slug of it. `MyST` marks one by opening the definition
/// list with a `{.glossary}` attribute block, and that block sits on the
/// directive opener's line when a directive holds the list.
pub(super) fn glossary_terms(node: &Node) -> Vec<String> {
    let content = text_content(node);
    if !opening_attribute(&content).is_some_and(|inner| has_class(inner, "glossary")) {
        return Vec::new();
    }
    terms(&content).into_iter().map(str::to_owned).collect()
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

/// The term MDN's own list syntax writes: an item ending in a nested list
/// whose every item opens with `: `, which MDN renders as a definition list.
/// It publishes the term's identity from the text of the element the term
/// opens with, or from the whole term when it opens with text. A macro it
/// opens with renders first, as an element showing the macro's display text.
pub(super) fn mdn_term(item: &Node) -> Option<Heading> {
    let (details, term) = item.children.split_last()?;
    let defines = !details.children.is_empty()
        && details.children.iter().all(|detail| {
            matches!(detail.kind, Kind::ListItem) && text_content(detail).starts_with(": ")
        });
    if !defines {
        return None;
    }
    let inlines = match term {
        [paragraph] if matches!(paragraph.kind, Kind::Paragraph) => paragraph.children.as_slice(),
        inlines => inlines,
    };
    let first = inlines.first()?;
    let opening = if let Kind::Text(value) = &first.kind {
        Some(value)
    } else {
        None
    };
    let text = match opening {
        Some(value) => {
            macro_display(value).unwrap_or_else(|| inlines.iter().map(text_content).collect())
        }
        None => text_content(first),
    };
    Some(Heading {
        text: text.trim().to_owned(),
        attribute: None,
        source: HeadingSource::DefinitionTerm,
        span: item.span,
    })
}

/// The text a `KumaScript` cross-reference macro the text opens with displays:
/// its second string argument when it names one, and its first otherwise,
/// with the two entities MDN writes in them decoded.
/// `{{cssxref("&lt;string&gt;")}}` displays `<string>`.
fn macro_display(text: &str) -> Option<String> {
    let (_name, rest) = text.strip_prefix("{{")?.split_once('(')?;
    let arguments = rest.split_once(")}}")?.0;
    let mut quoted = arguments.split('"').skip(1).step_by(2);
    let first = quoted.next()?;
    let shown = quoted
        .next()
        .filter(|second| !second.is_empty())
        .unwrap_or(first);
    Some(shown.replace("&lt;", "<").replace("&gt;", ">"))
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

/// The `Try it` heading MDN's `InteractiveExample` macro renders above the
/// example, under the identity its `en-US` title takes. `KumaScript` reads a
/// macro's name in any case.
pub(super) fn interactive_example(line: &str) -> Option<String> {
    let call = line.trim().strip_prefix("{{")?;
    call.get(..INTERACTIVE_EXAMPLE.len())?
        .eq_ignore_ascii_case(INTERACTIVE_EXAMPLE)
        .then(|| "try_it".to_owned())
}

const INTERACTIVE_EXAMPLE: &str = "InteractiveExample";

/// The identity a `MyST` target declares, `(name)=` alone on its line, which
/// the renderer writes onto the block that follows it. Sphinx stores the same
/// name as a label, so a `{ref}` role looks it up there too.
pub(super) fn myst_target(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix('(')?.strip_suffix(")=")?;
    (!inner.is_empty() && !inner.contains([')', '('])).then(|| inner.to_owned())
}

/// The identities attribute blocks declare for the inline constructs they
/// directly follow, which is the other half of what `attr_list` reads. A block
/// after a construct the parser built opens the text, because anything between
/// the two breaks the pairing, and something follows it, because a block that
/// ends its own block is the one the block rule already names. A bracketed
/// span carries its own `]`, so a block against one is read from the text it
/// sits in wherever that text falls.
pub(super) fn inline_attribute(text: &str, after_node: bool) -> Vec<String> {
    let mut found = Vec::new();
    if after_node
        && let Some((inner, rest)) = text.strip_prefix('{').and_then(|body| body.split_once('}'))
        && !rest.trim().is_empty()
    {
        found.extend(attribute_id(inner));
    }
    for tail in text.split("]{").skip(1) {
        found.extend(
            tail.split_once('}')
                .and_then(|(inner, _)| attribute_id(inner)),
        );
    }
    found
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
            | Kind::Footnote { .. }
            | Kind::UndefinedReference { .. }
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

/// Whether an attribute block names one class, in the `.name` spelling the
/// extension accepts beside an identity.
fn has_class(inner: &str, class: &str) -> bool {
    let mut rest = inner.strip_prefix(':').unwrap_or(inner).trim();
    while let Some((item, tail)) = attribute_item(rest) {
        rest = tail;
        if item.strip_prefix('.') == Some(class) {
            return true;
        }
    }
    false
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
