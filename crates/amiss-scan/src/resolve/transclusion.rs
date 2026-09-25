use std::borrow::Cow;
use std::collections::BTreeSet;

use amiss_wire::controls::GitMode;
use amiss_wire::extraction::{Heading, Transclusion, TransclusionKind, TransclusionRefusal};
use amiss_wire::model::{Adapter, RepoPath};

use crate::discovery::{DocumentStatus, SnapshotDiscovery};
use crate::resources::{Aggregate, ScanResources};

use crate::discovery::followed;
use crate::discovery::local_target;
use crate::discovery::site_root;
use crate::discovery::snippet_root;
use crate::route::SPHINX;

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
    /// The page a Sphinx build reads every nested include from, where there is one.
    sphinx_page: Option<RepoPath>,
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
        sphinx_page: (adapter == Adapter::Rst
            && site_root(snapshot, path.as_bytes(), &SPHINX).is_some())
        .then(|| path.clone()),
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

        let read_from = self.sphinx_page.as_ref().unwrap_or(path);
        let Some(target) = local_target(root.as_deref(), read_from, &transclusion.target) else {
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

/// Whether a template writes part of this document. A Hugo shortcode is
/// answered by a layout rather than by a file, so a page that calls one holds
/// headings and terms this engine cannot see, and its identity set is as far
/// from enumerable as a generator directive leaves one. A Hugo page naming a
/// `layout` and publishing no identity of its own is that template's output
/// whole, and Eleventy's Liquid in a heading is the same case. Outside a tree
/// that declares the generator the same spelling is the text it looks like.
pub(super) fn templated(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
    source: &Source<'_>,
) -> bool {
    let transclusions = source.transclusions;
    let bare = source.headings.is_empty()
        && source.html_anchors.is_empty()
        && source.declared_anchors.is_empty();
    let laid_out = bare
        && snapshot.document(document.as_bytes()).is_some_and(|record| {
            matches!(&record.status, DocumentStatus::Scanned(scanned) if scanned.publication.layout)
        });
    let hugo = || {
        crate::discovery::declared_root(
            snapshot,
            document.as_bytes(),
            crate::route::HUGO.declared_by,
        )
        .is_some()
    };
    (laid_out && hugo())
        || [
            (crate::anchor::HUGO_SHORTCODE, TransclusionRefusal::Template),
            (
                crate::anchor::ELEVENTY_TEMPLATE,
                TransclusionRefusal::Liquid,
            ),
        ]
        .iter()
        .any(|(rule, refusal)| {
            rule.adapters.contains(&adapter)
                && transclusions
                    .iter()
                    .any(|entry| entry.kind == Err(*refusal))
                && crate::discovery::declared_root(snapshot, document.as_bytes(), rule.declared_by)
                    .is_some()
        })
}
