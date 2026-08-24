use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::controls::{GitMode, TargetKind};
use amiss_wire::model::{Adapter, ForgeDialect, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{BlobTarget, Missing, Target, UnsupportedSemantics};

use crate::Error;
use crate::anchor::anchor_set;
use crate::discovery::SnapshotDiscovery;
use crate::document::classify;
use crate::resources::{Aggregate, ScanResources};

use super::content::Content;
use super::line::{line_fragment, line_resolution};
use super::transclusion::{Source, expand};
use super::{Intent, Resolution, Resolver, lookup};

/// A target's heading identities, built once and then answered from memory.
/// `Unevaluable` records that the parse was refused or unaffordable, which is
/// not the same as a document that publishes nothing.
#[derive(Debug)]
pub(super) enum Anchors {
    Unread,
    Unevaluable,
    Published(AnchorIndex),
    Partial(AnchorIndex),
}

#[derive(Debug)]
pub(super) struct AnchorIndex {
    identities: Vec<String>,
    typography: Option<BTreeMap<String, Option<usize>>>,
}

impl AnchorIndex {
    fn new(identities: BTreeSet<String>) -> Self {
        Self {
            identities: identities.into_iter().collect(),
            typography: None,
        }
    }

    fn typography_neighbor(&mut self, fragment: &str) -> Option<String> {
        if self.typography.is_none() {
            let mut typography = BTreeMap::new();
            for (index, identity) in self.identities.iter().enumerate() {
                typography
                    .entry(fold_typography(identity))
                    .and_modify(|matched| *matched = None)
                    .or_insert(Some(index));
            }
            self.typography = Some(typography);
        }
        let folded = fold_typography(fragment);
        self.typography
            .as_ref()?
            .get(&folded)
            .copied()
            .flatten()
            .and_then(|index| self.identities.get(index))
            .cloned()
    }
}

/// The fragment precedence on a located target: a tree carries none, the line
/// grammar wins where it applies, a document class is asked for the heading
/// identity, and everything else keeps its unsupported answer.
pub(super) fn fragment_resolution(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    entry: Target<RepoPath>,
    forge: Option<ForgeDialect>,
    decoded: &str,
) -> Result<Resolution, Error> {
    if mode == GitMode::Tree {
        return Ok(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::CodeFragment(entry),
        ));
    }
    let Target::Blob(blob) = entry else {
        return Err(Error::Internal);
    };
    if let Some(range) = line_fragment(forge, decoded) {
        return line_resolution(resolver, path, mode, blob, range);
    }
    match classify(path.as_bytes()) {
        Some(classification) => match classification.adapter() {
            Some(adapter) => anchor_resolution(resolver, path, mode, blob, adapter, decoded),
            None => Ok(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::Fragment(blob),
            )),
        },
        None => match resolver.snapshot.bound_adapter(path) {
            Some(adapter) => anchor_resolution(resolver, path, mode, blob, adapter, decoded),
            None => Ok(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::CodeFragment(Target::Blob(blob)),
            )),
        },
    }
}

/// Answers a heading anchor against the identities the known renderers would
/// publish for the target. A target this evaluation cannot read, parse, or
/// afford keeps the unsupported-semantics answer, so nothing is reported
/// missing on the strength of a parse that did not happen.
fn anchor_resolution(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    blob: BlobTarget<RepoPath>,
    adapter: Adapter,
    fragment: &str,
) -> Result<Resolution, Error> {
    let unsupported =
        Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(blob.clone()));
    let Some(cached) = resolver.cache.read.get_mut(path) else {
        return Err(Error::Internal);
    };
    if cached.mode != mode || cached.content.evidence() != blob.content {
        return Err(Error::Internal);
    }
    let Content::Ordinary {
        body,
        anchors: slot,
        ..
    } = &mut cached.content
    else {
        return Ok(unsupported);
    };

    if matches!(slot, Anchors::Unread) {
        let charged = resolver.scan.charge(
            Aggregate::HeadingAnchorBytes,
            u64::try_from(body.len()).unwrap_or(u64::MAX),
        );
        let allowance = resolver.scan.heading_anchor_allowance();
        *slot = match charged {
            Ok(()) => retained_source(resolver.snapshot, path, adapter)
                .map(|source| {
                    expanded_anchors(resolver.snapshot, resolver.scan, path, adapter, source)
                })
                .or_else(|| {
                    crate::scan::parse(adapter, body, allowance)
                        .ok()
                        .and_then(|analysis| analysis.extraction)
                        .map(|extraction| {
                            expanded_anchors(
                                resolver.snapshot,
                                resolver.scan,
                                path,
                                adapter,
                                Source {
                                    headings: &extraction.headings,
                                    html_anchors: &extraction.html_anchors,
                                    declared_anchors: &extraction.declared_anchors,
                                    transclusions: &extraction.transclusions,
                                },
                            )
                        })
                })
                .unwrap_or(Anchors::Unevaluable),
            Err(_crossing) => Anchors::Unevaluable,
        };
    }
    let (index, complete) = match slot {
        Anchors::Published(index) => (index, true),
        Anchors::Partial(index) => (index, false),
        Anchors::Unread | Anchors::Unevaluable => return Ok(unsupported),
    };
    if index
        .identities
        .binary_search_by(|identity| identity.as_str().cmp(fragment))
        .is_ok()
    {
        return Ok(Resolution::Resolved(Target::Blob(blob)));
    }
    if !complete {
        return Ok(unsupported);
    }
    let near = index.typography_neighbor(fragment);
    Ok(Resolution::Missing(Missing::HeadingAnchorNotFound {
        path: path.clone(),
        near,
    }))
}

fn expanded_anchors(
    snapshot: &SnapshotDiscovery,
    scan: &mut ScanResources,
    path: &RepoPath,
    adapter: Adapter,
    source: Source<'_>,
) -> Anchors {
    let expanded = expand(snapshot, scan, path, adapter, source);
    let identities = AnchorIndex::new(anchor_set(
        expanded.headings.as_ref(),
        expanded.html_anchors.as_ref(),
        expanded.declared_anchors.as_ref(),
    ));
    if expanded.complete {
        Anchors::Published(identities)
    } else {
        Anchors::Partial(identities)
    }
}

/// The comparison key for a heading identity: the two spellings the pinned
/// renderer rules disagree on, case and the separator character, folded away.
/// Duplicate suffixes ride the separator, so `x_1` and `x-1` fold together.
fn fold_typography(text: &str) -> String {
    text.to_lowercase().replace('_', "-")
}

impl Resolver<'_> {
    /// Answers a Sphinx `:ref:` against the labels the snapshot's documents
    /// declare, delegating a unique declaration to ordinary target lookup.
    pub(crate) fn resolve_label(
        &mut self,
        label: &str,
        semantic: crate::semantic::View<'_>,
    ) -> Result<(Intent, Resolution, Option<String>), Error> {
        let intent = Intent {
            kind: IntentKind::Label,
            repository_path: None,
            target_kind: None,
            external_scheme: None,
            query: None,
            fragment: Some(label.to_owned()),
        };
        let normalized = amiss_rst::normalized_label(label);
        let mut external_destination = None;
        let resolution = match self.snapshot.labels.get(&normalized) {
            None if label.contains(':') => {
                Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
            }
            None => match semantic.labels.get(&normalized) {
                Some(crate::semantic::InventoryLabel::Unique(destination)) => {
                    external_destination = Some(destination.clone());
                    Resolution::External(
                        amiss_wire::resolution::ExternalReference::IntersphinxInventory,
                    )
                }
                Some(crate::semantic::InventoryLabel::Ambiguous) => {
                    Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
                }
                None => Resolution::Missing(Missing::LabelNotDeclared),
            },
            Some(crate::discovery::LabelState::Duplicated) => {
                Resolution::UnsupportedSemantics(UnsupportedSemantics::DuplicateLabel)
            }
            Some(crate::discovery::LabelState::Declared(owner)) => {
                let owner = owner.clone();
                lookup(self, &owner, TargetKind::Blob, None, None, None)?
            }
        };
        Ok((intent, resolution, external_destination))
    }
}

/// The anchor inputs discovery already parsed for an in-set scanned target
/// under the same adapter; a mismatch falls back to the target-body parse.
fn retained_source<'a>(
    snapshot: &'a SnapshotDiscovery,
    path: &RepoPath,
    adapter: Adapter,
) -> Option<Source<'a>> {
    let record = snapshot.document(path.as_bytes())?;
    if record.adapter != Some(adapter) {
        return None;
    }
    let crate::discovery::DocumentStatus::Scanned(scanned) = &record.status else {
        return None;
    };
    let source = scanned.anchor_source.as_ref()?;
    Some(Source {
        headings: &source.headings,
        html_anchors: &source.html_anchors,
        declared_anchors: &scanned.declared_anchors,
        transclusions: &source.transclusions,
    })
}
