use std::borrow::Cow;
use std::collections::BTreeSet;

use amiss_wire::controls::{GitMode, TargetKind};
use amiss_wire::extraction::{Heading, Transclusion, TransclusionKind};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::uri::scheme;

use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resources::{Aggregate, ScanResources};

use super::syntax::normalized_native_path;

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
        complete: adapter == Adapter::Rst,
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
        for transclusion in source.transclusions {
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
        let Ok(kind) = transclusion.kind else {
            self.complete = false;
            return;
        };

        let Some(target) = local_target(path, &transclusion.target) else {
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

fn local_target(document: &RepoPath, target: &str) -> Option<RepoPath> {
    if target.starts_with('/') || target.contains(['%', '?', '#']) || scheme(target).is_some() {
        return None;
    }
    normalized_native_path(document, false, target)
        .ok()
        .filter(|(_, kind)| *kind != TargetKind::Tree)
        .map(|(path, _)| path)
}
