use std::collections::{BTreeMap, BTreeSet, HashMap};

use amiss_wire::extraction::{Fault, GovernedDefinition, SemanticCodeBlock, Work};

use super::source::definition_destination;
use crate::tree::{Kind, Node};

/// A definition is reserved exactly when its decoded label scalars, before
/// `CommonMark` whitespace and case normalization, begin with lowercase ASCII
/// `amiss:`.
pub const RESERVED_LABEL_PREFIX: &str = "amiss:";

pub(super) struct Definition {
    pub(super) url: String,
    pub(super) raw: String,
    pub(super) reserved: bool,
    pub(super) span: (usize, usize),
}

pub(super) type Definitions = HashMap<usize, Definition>;
pub(super) type OrphanDefinitions = BTreeMap<(usize, usize), (String, String)>;

pub(super) struct CollectedDefinitions {
    pub(super) resolved: Definitions,
    pub(super) governed: Vec<GovernedDefinition>,
    pub(super) orphans: OrphanDefinitions,
    pub(super) work: Work,
}

pub(super) fn definitions(tree: &Node, suffix: &str) -> Result<CollectedDefinitions, Fault> {
    let mut out = Vec::new();
    let mut governed = Vec::new();
    let mut code_blocks = Vec::new();
    let mut used = BTreeSet::new();
    let mut work = Work {
        nodes: 0,
        nesting: 0,
    };
    let mut stack = vec![(tree, 1_u64)];
    while let Some((node, depth)) = stack.pop() {
        work.nodes = work.nodes.saturating_add(1);
        work.nesting = work.nesting.max(depth);
        match &node.kind {
            Kind::LinkReference(reference) | Kind::ImageReference(reference) => {
                used.insert(reference.key);
            }
            Kind::CodeBlock(value) => code_blocks.push((node.span, value.as_str())),
            Kind::Definition(definition) => {
                let (raw, angled) = definition_destination(suffix, node.span)?;
                let reserved = definition.label.starts_with(RESERVED_LABEL_PREFIX);
                if reserved {
                    governed.push(GovernedDefinition {
                        span: node.span,
                        url: definition.url.clone(),
                        title: definition.title.clone(),
                        label: definition.label.clone(),
                        angled,
                        previous_code: None,
                    });
                }
                out.push((
                    node.span,
                    definition.key,
                    Definition {
                        url: definition.url.clone(),
                        raw,
                        reserved,
                        span: node.span,
                    },
                ));
            }
            Kind::Root
            | Kind::Paragraph
            | Kind::Heading
            | Kind::ListItem
            | Kind::TableCell
            | Kind::Html
            | Kind::Mdx { .. }
            | Kind::MdxElement { .. }
            | Kind::MdxEsm(_)
            | Kind::Text(_)
            | Kind::InlineCode(_)
            | Kind::Link { .. }
            | Kind::Image { .. }
            | Kind::Footnote { .. }
            | Kind::Other => {}
        }
        let below = depth.saturating_add(1);
        stack.extend(node.children.iter().rev().map(|child| (child, below)));
    }
    out.sort_by_key(|(span, _, _)| *span);
    governed.sort_by_key(|definition| definition.span);
    code_blocks.sort_by_key(|(span, _)| *span);
    for definition in &mut governed {
        let before = code_blocks.partition_point(|(span, _)| span.1 <= definition.span.0);
        let Some((span, value)) = before
            .checked_sub(1)
            .and_then(|index| code_blocks.get(index))
        else {
            continue;
        };
        let adjacent = suffix
            .as_bytes()
            .get(span.1..definition.span.0)
            .is_some_and(|gap| gap.iter().all(u8::is_ascii_whitespace));
        if adjacent {
            definition.previous_code = Some(SemanticCodeBlock {
                span: *span,
                value: (*value).to_owned(),
            });
        }
    }
    let mut resolved = HashMap::with_capacity(out.len());
    let mut orphans = BTreeMap::new();
    for (span, key, definition) in out {
        if used.contains(&key) {
            resolved.entry(key).or_insert(definition);
        } else if !definition.reserved {
            orphans.insert(span, (definition.raw, definition.url));
        }
    }
    Ok(CollectedDefinitions {
        resolved,
        governed,
        orphans,
        work,
    })
}
