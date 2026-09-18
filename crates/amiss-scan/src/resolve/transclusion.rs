use std::borrow::Cow;
use std::collections::BTreeSet;

use amiss_wire::controls::{GitMode, TargetKind};
use amiss_wire::extraction::{Heading, Transclusion, TransclusionKind, TransclusionRefusal};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::uri::scheme;

use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resources::{Aggregate, ScanResources};

use super::syntax::{normalized_native_path, normalized_path_under};

#[derive(Clone, Copy)]
pub(super) struct Source<'a> {
    pub(super) headings: &'a [Heading],
    pub(super) html_anchors: &'a [String],
    pub(super) declared_anchors: &'a [String],
    pub(super) transclusions: &'a [Transclusion],
}

pub(super) struct Expanded<'a> {
    pub(super) headings: Cow<'a, [Heading]>,
    pub(super) html_anchors: Cow<'a, [String]>,
    pub(super) declared_anchors: Cow<'a, [String]>,
    pub(super) complete: bool,
}

struct Expansion<'snapshot, 'scan> {
    snapshot: &'snapshot SnapshotDiscovery,
    scan: &'scan mut ScanResources,
    adapter: Adapter,
    stack: BTreeSet<RepoPath>,
    edges: u64,
    headings: Vec<Heading>,
    html_anchors: Vec<String>,
    declared_anchors: Vec<String>,
    complete: bool,
}

pub(super) fn expand<'source>(
    snapshot: &SnapshotDiscovery,
    scan: &mut ScanResources,
    path: &RepoPath,
    adapter: Adapter,
    source: Source<'source>,
) -> Expanded<'source> {
    if source.transclusions.is_empty() {
        return Expanded {
            headings: Cow::Borrowed(source.headings),
            html_anchors: Cow::Borrowed(source.html_anchors),
            declared_anchors: Cow::Borrowed(source.declared_anchors),
            complete: true,
        };
    }
    let mut expansion = Expansion {
        snapshot,
        scan,
        adapter,
        stack: BTreeSet::from([path.clone()]),
        edges: 0,
        headings: Vec::new(),
        html_anchors: Vec::new(),
        declared_anchors: Vec::new(),
        complete: matches!(adapter, Adapter::Markdown | Adapter::Mdx | Adapter::Rst),
    };
    expansion.append(path, source, 0);
    Expanded {
        headings: Cow::Owned(expansion.headings),
        html_anchors: Cow::Owned(expansion.html_anchors),
        declared_anchors: Cow::Owned(expansion.declared_anchors),
        complete: expansion.complete,
    }
}

impl Expansion<'_, '_> {
    fn append(&mut self, path: &RepoPath, source: Source<'_>, depth: u64) {
        self.html_anchors
            .extend(source.html_anchors.iter().cloned());
        self.declared_anchors
            .extend(source.declared_anchors.iter().cloned());

        let mut heading = 0;
        for transclusion in followed(source.transclusions) {
            while source
                .headings
                .get(heading)
                .is_some_and(|candidate| candidate.span.0 < transclusion.span.0)
            {
                if let Some(candidate) = source.headings.get(heading) {
                    self.headings.push(candidate.clone());
                }
                heading = heading.saturating_add(1);
            }
            self.follow(path, transclusion, depth);
        }
        self.headings.extend(
            source
                .headings
                .get(heading..)
                .unwrap_or_default()
                .iter()
                .cloned(),
        );
    }

    fn follow(&mut self, path: &RepoPath, transclusion: &Transclusion, depth: u64) {
        self.edges = self.edges.saturating_add(1);
        if self.edges > self.scan.limits().references_per_document
            || depth >= self.scan.limits().parser_nesting
        {
            self.complete = false;
            return;
        }
        let root = snippet_root(self.snapshot, self.adapter, path);
        // A snippet line in a tree no mkdocs declares is ordinary text.
        if self.adapter == Adapter::Markdown && root.is_none() {
            return;
        }
        let Ok(kind) = transclusion.kind else {
            self.complete = false;
            return;
        };

        let Some(target) = local_target(root.as_deref(), path, &transclusion.target) else {
            self.complete = false;
            return;
        };
        match kind {
            TransclusionKind::Literal => {
                if !self.snapshot.entries.get(&target).is_some_and(|(mode, _)| {
                    matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile)
                }) {
                    self.complete = false;
                }
                return;
            }
            TransclusionKind::Parsed => {}
        }
        if !self.stack.insert(target.clone()) {
            self.complete = false;
            return;
        }

        let source = self
            .snapshot
            .document(target.as_bytes())
            .and_then(|record| {
                if record.adapter != Some(self.adapter) {
                    return None;
                }
                let DocumentStatus::Scanned(scanned) = &record.status else {
                    return None;
                };
                let anchors = scanned.anchor_source.as_ref()?;
                self.scan
                    .charge(Aggregate::HeadingAnchorBytes, record.byte_count)
                    .ok()?;
                Some(Source {
                    headings: &anchors.headings,
                    html_anchors: &anchors.html_anchors,
                    declared_anchors: &scanned.declared_anchors,
                    transclusions: &anchors.transclusions,
                })
            });
        if let Some(source) = source {
            self.append(&target, source, depth.saturating_add(1));
        } else {
            self.complete = false;
        }
        self.stack.remove(&target);
    }
}

/// The includes an expansion walks. A call a template answers is no edge at
/// all, since nothing in the tree stands in for what it writes, so `templated`
/// answers it instead and the walk passes it by.
fn followed(transclusions: &[Transclusion]) -> Vec<&Transclusion> {
    transclusions
        .iter()
        .filter(|entry| entry.kind != Err(TransclusionRefusal::Template))
        .collect()
}

/// Whether a template writes part of this document. A Hugo shortcode is
/// answered by a layout rather than by a file, so a page that calls one holds
/// headings and terms this engine cannot see, and its identity set is as far
/// from enumerable as a generator directive leaves one. Outside a tree that
/// declares Hugo the same line is the text it looks like.
pub(super) fn templated(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    transclusions: &[Transclusion],
) -> bool {
    crate::anchor::HUGO_SHORTCODE.adapters.contains(&adapter)
        && transclusions
            .iter()
            .any(|entry| entry.kind == Err(TransclusionRefusal::Template))
        && crate::route::declared_root(snapshot, document.as_bytes(), &crate::route::HUGO).is_some()
}

/// The documents one document renders in place of its own includes, which is
/// the edge the Sphinx walk follows to decide what a tree parses. A call a
/// template answers names no file at all, so it is no edge here either; an
/// option block still renders part of the named file, so that one is.
pub(crate) fn included_documents(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
) -> Vec<RepoPath> {
    let Some(DocumentStatus::Scanned(scanned)) = snapshot
        .document(document.as_bytes())
        .map(|record| &record.status)
    else {
        return Vec::new();
    };
    let Some(source) = scanned.anchor_source.as_ref() else {
        return Vec::new();
    };
    let root = snippet_root(snapshot, Adapter::Markdown, document);
    followed(&source.transclusions)
        .into_iter()
        .filter(|entry| entry.kind != Ok(TransclusionKind::Literal))
        .filter_map(|entry| local_target(root.as_deref(), document, &entry.target))
        .collect()
}

fn local_target(root: Option<&[u8]>, document: &RepoPath, target: &str) -> Option<RepoPath> {
    if target.starts_with('/') || target.contains(['%', '?', '#']) || scheme(target).is_some() {
        return None;
    }
    let located = match root {
        Some(root) => normalized_path_under(root, false, target),
        None => normalized_native_path(document, false, target),
    };
    located
        .ok()
        .filter(|(_, kind)| *kind != TargetKind::Tree)
        .map(|(path, _)| path)
}

/// The directory a mkdocs snippet is resolved from, which is the one holding
/// the file that declares mkdocs above the document rather than the document's
/// own. Every other include stays relative to the file that writes it, so no
/// root applies to one.
fn snippet_root(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
) -> Option<Vec<u8>> {
    if adapter != Adapter::Markdown {
        return None;
    }
    crate::route::declared_root(snapshot, document.as_bytes(), &crate::route::MKDOCS)
}
