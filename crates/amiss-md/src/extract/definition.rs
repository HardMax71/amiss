use std::collections::{BTreeMap, BTreeSet, HashMap};

use amiss_wire::extraction::{Fault, GovernedDefinition, Work};
use markdown::mdast::Node;

use super::source::definition_destination;
use super::span::span_of;

/// A definition is reserved exactly when its decoded label scalars, before
/// `CommonMark` whitespace and case normalization, begin with lowercase ASCII
/// `amiss:`.
pub const RESERVED_LABEL_PREFIX: &str = "amiss:";

pub(super) struct Definition {
    pub(super) url: String,
    pub(super) raw: String,
    pub(super) reserved: bool,
}

pub(super) type Definitions = HashMap<String, Definition>;
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
    let mut used = BTreeSet::new();
    let mut work = Work {
        nodes: 0,
        nesting: 0,
    };
    let mut stack = vec![(tree, 1_u64)];
    while let Some((node, depth)) = stack.pop() {
        work.nodes = work.nodes.saturating_add(1);
        work.nesting = work.nesting.max(depth);
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
                definition.identifier.clone(),
                Definition {
                    url: definition.url.clone(),
                    raw,
                    reserved,
                },
            ));
        }
        if let Some(children) = node.children() {
            let below = depth.saturating_add(1);
            stack.extend(children.iter().rev().map(|child| (child, below)));
        }
    }
    out.sort_by_key(|(span, _, _)| *span);
    governed.sort_by_key(|definition| definition.span);
    let mut resolved = HashMap::with_capacity(out.len());
    let mut orphans = BTreeMap::new();
    for (span, identifier, definition) in out {
        if used.contains(&identifier) {
            resolved.entry(identifier).or_insert(definition);
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
