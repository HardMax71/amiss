use std::collections::HashMap;

use amiss_wire::extraction::Fault;
use markdown::mdast;

use super::{Definition, Kind, Node, Reference, ReferenceForm};

/// Converts the mdast tree the MDX grammar builds. Iterative because a hostile
/// document may nest deeper than the stack allows.
///
/// # Errors
///
/// `InvalidSourceSpan` for a node without a position, `ParserError` for a
/// reference whose identifier has no definition.
pub(crate) fn from_mdast(tree: &mdast::Node) -> Result<Node, Fault> {
    let winners = winners(tree)?;
    let mut stack = vec![Pending {
        node: convert(tree, &winners)?,
        children: children_of(tree),
        next: 0,
    }];
    loop {
        let top = stack.last_mut().ok_or(Fault::ParserError)?;
        if let Some(child) = top.children.get(top.next) {
            top.next = top.next.saturating_add(1);
            let node = convert(child, &winners)?;
            let children = if matches!(node.kind, Kind::Image { .. }) {
                &[]
            } else {
                children_of(child)
            };
            stack.push(Pending {
                node,
                children,
                next: 0,
            });
            continue;
        }
        let done = stack.pop().ok_or(Fault::ParserError)?;
        match stack.last_mut() {
            Some(parent) => parent.node.children.push(done.node),
            None => return Ok(done.node),
        }
    }
}

struct Pending<'a> {
    node: Node,
    children: &'a [mdast::Node],
    next: usize,
}

fn children_of(node: &mdast::Node) -> &[mdast::Node] {
    node.children().map_or(&[], Vec::as_slice)
}

/// The span start of the first definition, in document order, for every
/// identifier the document defines.
fn winners(tree: &mdast::Node) -> Result<HashMap<String, usize>, Fault> {
    let mut winners = HashMap::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        if let mdast::Node::Definition(definition) = node {
            let span = span_of(node)?;
            winners
                .entry(definition.identifier.clone())
                .or_insert(span.0);
        }
        stack.extend(children_of(node).iter().rev());
    }
    Ok(winners)
}

fn span_of(node: &mdast::Node) -> Result<(usize, usize), Fault> {
    let position = node.position().ok_or(Fault::InvalidSourceSpan)?;
    let span = (position.start.offset, position.end.offset);
    if span.0 > span.1 {
        return Err(Fault::InvalidSourceSpan);
    }
    Ok(span)
}

fn convert(node: &mdast::Node, winners: &HashMap<String, usize>) -> Result<Node, Fault> {
    let span = span_of(node)?;
    let kind = match node {
        mdast::Node::Root(_) => Kind::Root,
        mdast::Node::Paragraph(_) => Kind::Paragraph,
        mdast::Node::Heading(_) => Kind::Heading,
        mdast::Node::ListItem(_) => Kind::ListItem,
        mdast::Node::TableCell(_) => Kind::TableCell,
        mdast::Node::Html(_) => Kind::Html,
        mdast::Node::MdxjsEsm(esm) => Kind::MdxEsm(esm.value.clone()),
        mdast::Node::MdxFlowExpression(_) => Kind::Mdx { expression: None },
        mdast::Node::MdxJsxFlowElement(element) => {
            element_kind(element.name.as_deref(), &element.attributes)
        }
        mdast::Node::MdxJsxTextElement(element) => {
            element_kind(element.name.as_deref(), &element.attributes)
        }
        mdast::Node::MdxTextExpression(expression) => Kind::Mdx {
            expression: Some(expression.value.clone()),
        },
        mdast::Node::Text(text) => Kind::Text(text.value.clone()),
        mdast::Node::InlineCode(code) => Kind::InlineCode(code.value.clone()),
        mdast::Node::InlineMath(math) => Kind::InlineCode(math.value.clone()),
        mdast::Node::Math(math) => Kind::InlineCode(math.value.clone()),
        mdast::Node::Code(code) => Kind::CodeBlock(code.value.clone()),
        mdast::Node::Link(link) => Kind::Link {
            url: link.url.clone(),
        },
        mdast::Node::Image(image) => Kind::Image {
            url: image.url.clone(),
        },
        mdast::Node::LinkReference(reference) => Kind::LinkReference(resolved(
            winners,
            &reference.identifier,
            reference.reference_kind,
        )?),
        mdast::Node::ImageReference(reference) => Kind::ImageReference(resolved(
            winners,
            &reference.identifier,
            reference.reference_kind,
        )?),
        mdast::Node::Definition(definition) => Kind::Definition(Definition {
            key: winners
                .get(&definition.identifier)
                .copied()
                .unwrap_or(span.0),
            label: definition
                .label
                .clone()
                .unwrap_or_else(|| definition.identifier.clone()),
            url: definition.url.clone(),
            title: definition.title.clone(),
        }),
        mdast::Node::Blockquote(_)
        | mdast::Node::List(_)
        | mdast::Node::Table(_)
        | mdast::Node::TableRow(_)
        | mdast::Node::Delete(_)
        | mdast::Node::Emphasis(_)
        | mdast::Node::Strong(_)
        | mdast::Node::FootnoteDefinition(_)
        | mdast::Node::FootnoteReference(_)
        | mdast::Node::Break(_)
        | mdast::Node::ThematicBreak(_)
        | mdast::Node::Toml(_)
        | mdast::Node::Yaml(_) => Kind::Other,
    };
    Ok(Node::leaf(kind, span))
}

/// An element's tag name and the identity it sets as a literal, from `id` or
/// from `name`, whichever it writes first. One computed by an expression is a
/// value this engine cannot read, so the element carries none.
fn element_kind(name: Option<&str>, attributes: &[mdast::AttributeContent]) -> Kind {
    let id = attributes.iter().find_map(|attribute| match attribute {
        mdast::AttributeContent::Property(property)
            if matches!(property.name.as_str(), "id" | "name") =>
        {
            match property.value.as_ref() {
                Some(mdast::AttributeValue::Literal(value)) => Some(value.clone()),
                Some(mdast::AttributeValue::Expression(_)) | None => None,
            }
        }
        mdast::AttributeContent::Property(_) | mdast::AttributeContent::Expression(_) => None,
    });
    Kind::MdxElement {
        name: name.map(str::to_owned),
        id,
    }
}

fn resolved(
    winners: &HashMap<String, usize>,
    identifier: &str,
    kind: mdast::ReferenceKind,
) -> Result<Reference, Fault> {
    let key = winners.get(identifier).copied().ok_or(Fault::ParserError)?;
    let form = match kind {
        mdast::ReferenceKind::Full => ReferenceForm::Full,
        mdast::ReferenceKind::Collapsed => ReferenceForm::Collapsed,
        mdast::ReferenceKind::Shortcut => ReferenceForm::Shortcut,
    };
    Ok(Reference { key, form })
}
