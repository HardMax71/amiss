use sha2::Digest as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_git::{GitResources, ObjectKind, Repository, TreeEntry, ValueCap, parse_tree};
use amiss_md::Fault;
use amiss_wire::controls::{GitMode, ResourceName, SourceConstruct};
use amiss_wire::model::{Adapter, Oid, RepoPath};

use crate::document::{Classification, classify, excluded_by_built_in, native_adapter};
use crate::policy::Includes;
use crate::resources::{ScanIdentity, ScanMemo, ScanResources, crossing};
use crate::scan::{Scanned, ScannedOccurrence, replay_scan_charges, scan_bytes};
use crate::{Error, GitDefect, lfs};

/// The deliberate object and format boundaries a discovered document side can
/// sit behind without failing the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedKind {
    Symlink,
    Gitlink,
    LfsPointer,
    Format,
    Undecodable,
    Ceiling,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Scanned(Arc<Scanned>),
    ExcludedBuiltIn,
    Unsupported(UnsupportedKind),
    Failed(Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRecord {
    pub path: RepoPath,
    pub classification: Classification,
    pub adapter: Option<Adapter>,
    pub status: DocumentStatus,
    pub oid: Oid,
    pub mode: GitMode,
    pub byte_count: u64,
    pub raw_digest: Option<amiss_wire::model::Digest>,
}

/// What a Sphinx declaration decides once the walk is over: which documents
/// read a cross-reference role, and whose declared names join the label table
/// a role is answered from. Neither question can be asked during the walk,
/// because `conf.py` sits beside documents the walk reaches before it.
///
/// The label table is every name a document whose profile reads roles
/// declares, which is the nearest tree-derivable stand-in for the index Sphinx
/// builds for `:ref:`. A name declared twice is marked rather than guessed
/// between. A document no declaration governs keeps no role, so a brace before
/// a code span there is the prose it looks like.
fn settle_roles(scan: &mut ScanResources, discovery: &mut SnapshotDiscovery) -> Result<(), Error> {
    discovery.sphinx_included = sphinx_included(discovery);
    let governed: Vec<bool> = discovery
        .documents
        .iter()
        .map(|record| {
            record.adapter == Some(Adapter::Markdown)
                && crate::anchor::sphinx_governed(discovery, &record.path)
        })
        .collect();
    let SnapshotDiscovery {
        documents, labels, ..
    } = discovery;
    for (record, governed) in documents.iter_mut().zip(governed) {
        let DocumentStatus::Scanned(scanned) = &mut record.status else {
            continue;
        };
        let reads_roles = match scanned.adapter {
            Adapter::Rst => true,
            Adapter::Markdown => governed,
            Adapter::Mdx | Adapter::AsciiDoc | Adapter::PlainAdvisory => false,
        };
        if !reads_roles {
            if scanned.occurrences.iter().any(role_occurrence) {
                Arc::make_mut(scanned)
                    .occurrences
                    .retain(|entry| !role_occurrence(entry));
            }
            continue;
        }
        for label in &scanned.declared_anchors {
            scan.charge_label()?;
            labels
                .entry(amiss_rst::normalized_label(label))
                .and_modify(|state| *state = LabelState::Duplicated)
                .or_insert_with(|| LabelState::Declared(record.path.clone()));
        }
    }
    Ok(())
}

/// What every descriptor in the snapshot declares out of its own contents: an
/// Antora component descriptor names the component its root contributes to,
/// since Antora assembles one component from every source root naming it, a
/// Sphinx configuration names the suffixes its own root reads, and a router
/// declaration names the router publishing the directory it sits in, none of
/// which anything on a path shows. Each is read once here and the resolver
/// answers from what they say. A descriptor past the document ceiling or
/// unreadable declares nothing, which leaves its root standing alone as it
/// did before.
fn descriptors(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    discovery: &SnapshotDiscovery,
) -> Result<Declared, Error> {
    let wanted: Vec<(RepoPath, Oid)> = discovery
        .entries
        .iter()
        .filter(|(path, (mode, _))| {
            matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile) && declaring(path)
        })
        .map(|(path, (_, oid))| (path.clone(), oid.clone()))
        .collect();
    let mut declared = Declared::default();
    for (path, oid) in wanted {
        let cap = ValueCap {
            resource: ResourceName::DocumentBlobBytes,
            limit: scan.limits().document_blob_bytes,
        };
        let Ok(object) = repo.read_expected_capped(git, &oid, ObjectKind::Blob, cap) else {
            continue;
        };
        scan.charge_document_bytes(u64::try_from(object.body.len()).unwrap_or(u64::MAX))?;
        record_declaration(&mut declared, path, &object.body);
    }
    Ok(declared)
}

/// Whether a path is one of the four files read for what it declares. A
/// Sphinx configuration under a tree the scan excludes belongs to a fixture
/// rather than to a site the repository publishes, the same reading that keeps
/// Sphinx's own 176 test roots from declaring anything.
fn declaring(path: &RepoPath) -> bool {
    let router = path
        .as_bytes()
        .strip_suffix(crate::route::ROUTER_DECLARATION.as_bytes())
        .is_some_and(|above| above.is_empty() || above.ends_with(b"/"));
    router
        || crate::route::declares(&crate::route::ANTORA, path)
        || configures(path)
        || publishes(path)
}

fn configures(path: &RepoPath) -> bool {
    crate::route::declares(&crate::route::SPHINX, path) && !excluded_by_built_in(path.as_bytes())
}

fn publishes(path: &RepoPath) -> bool {
    crate::route::declares(&crate::route::HUGO, path) && !excluded_by_built_in(path.as_bytes())
}

/// What one descriptor's own bytes say, under the reading its name selects.
fn record_declaration(declared: &mut Declared, path: RepoPath, body: &[u8]) {
    if crate::route::declares(&crate::route::ANTORA, &path) {
        if let Some(component) = crate::route::antora_descriptor(body) {
            declared.antora_components.insert(path, component);
        }
    } else if configures(&path) {
        let suffixes = crate::route::source_suffixes(body);
        if !suffixes.is_empty() {
            declared
                .source_suffixes
                .insert(crate::route::directory(path.as_bytes()).to_vec(), suffixes);
        }
    } else if publishes(&path) {
        let project = crate::route::directory(path.as_bytes());
        let roots = crate::route::hugo_project(body)
            .into_iter()
            .map(|(root, base)| (crate::route::join(project, root.as_bytes()), base))
            .collect();
        declared.published_roots.insert(project.to_vec(), roots);
    } else if let Some(router) = crate::route::declared_router(body) {
        declared.routers.insert(path, router);
    }
}

/// What one pass over the descriptors read: the Antora components, the
/// suffixes each Sphinx root reads its own sources under, and the routers a
/// repository declares for its own directories.
#[derive(Default)]
struct Declared {
    antora_components: BTreeMap<RepoPath, (String, bool)>,
    source_suffixes: BTreeMap<Vec<u8>, BTreeSet<String>>,
    routers: BTreeMap<RepoPath, (String, Option<String>)>,
    published_roots: BTreeMap<Vec<u8>, Vec<(Vec<u8>, String)>>,
}

/// Every document a page of a Sphinx tree renders in place of an include, and
/// every document those reach in turn. Sphinx parses an included file as part
/// of the page holding the directive, so that file writes `MyST` wherever in
/// the tree it sits, which is how a changelog beside the repository root names
/// labels a page under `conf.py` declares. The walk seeds on the pages a
/// declaration governs by position, the only answer `sphinx_governed` has while
/// the set this builds is still empty.
fn sphinx_included(discovery: &SnapshotDiscovery) -> BTreeSet<RepoPath> {
    let markdown = |record: &DocumentRecord| record.adapter == Some(Adapter::Markdown);
    let mut frontier: Vec<RepoPath> = discovery
        .documents
        .iter()
        .filter(|record| {
            markdown(record) && crate::anchor::sphinx_governed(discovery, &record.path)
        })
        .map(|record| record.path.clone())
        .collect();
    let mut found = BTreeSet::new();
    while let Some(document) = frontier.pop() {
        for target in crate::resolve::included_documents(discovery, &document) {
            if discovery.document(target.as_bytes()).is_some_and(markdown)
                && found.insert(target.clone())
            {
                frontier.push(target);
            }
        }
    }
    found
}

fn role_occurrence(entry: &ScannedOccurrence) -> bool {
    matches!(
        entry.occurrence.construct,
        SourceConstruct::RstDocRole | SourceConstruct::RstRefRole
    )
}

/// One refused path: the defect, and the raw bytes of the name that tripped
/// it, when the frozen hex field can hold them. An over-length name records
/// no bytes, because its crossing row already carries both figures and the
/// field caps at the path ceiling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathDefect {
    pub error: Error,
    pub raw: Option<Vec<u8>>,
}

/// One side's complete discovery: every classified path strictly increasing
/// and unique in repository byte order with its outcome, the count of non-tree
/// entries outside the document set, the entries walked, and the path-level
/// defects that never became a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDiscovery {
    pub documents: Vec<DocumentRecord>,
    pub outside_document_set: u64,
    pub tree_entries: u64,
    pub path_defects: Vec<PathDefect>,
    pub entries: BTreeMap<RepoPath, (GitMode, Oid)>,
    pub labels: BTreeMap<String, LabelState>,
    pub published_routes: BTreeMap<RepoPath, RepoPath>,
    /// Each page URL a document declares it moved away from, by that URL,
    /// against the document that declares it.
    pub redirect_routes: BTreeMap<RepoPath, RepoPath>,
    pub sole_sites: BTreeMap<&'static str, Vec<u8>>,
    pub sphinx_included: BTreeSet<RepoPath>,
    /// Each `antora.yml` the tree holds, by its own path, against the
    /// component name it declares and whether it reserves an `ext` block.
    pub antora_components: BTreeMap<RepoPath, (String, bool)>,
    /// Each Sphinx root whose `conf.py` names the suffixes it reads, by the
    /// directory holding that file.
    pub source_suffixes: BTreeMap<Vec<u8>, BTreeSet<String>>,
    /// Each router declaration the tree holds, by its own path, against the
    /// router it names for the directory it sits in.
    pub declared_routers: BTreeMap<RepoPath, (String, Option<String>)>,
    /// Each content root a generator's own configuration names, and the path
    /// its site is served under.
    pub published_roots: BTreeMap<Vec<u8>, Vec<(Vec<u8>, String)>>,
}

/// What the snapshot's documents say about one `.. _name:` label: the one
/// declaring document, or the fact that more than one claims it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelState {
    Declared(RepoPath),
    Duplicated,
}

/// What a path names in a snapshot.
///
/// A commit tree carries a directory as an entry of its own, with a mode and a
/// tree object behind it. A Git index carries only file paths, and a directory
/// in it is exactly a path that some entry lives under. Both snapshots are
/// asked the same structural question, so both must answer it the same way, or
/// the same content resolves differently through `--candidate` than through
/// `--index`. A tree target has no content to read, so the missing tree
/// identity is never wanted.
#[derive(Debug)]
pub enum Located<'snapshot> {
    Entry(GitMode, &'snapshot Oid),
    ImpliedTree,
}

impl SnapshotDiscovery {
    /// The policy-bound adapter for a path no built-in row classifies.
    #[must_use]
    pub fn bound_adapter(&self, path: &RepoPath) -> Option<Adapter> {
        self.document(path.as_bytes())
            .filter(|record| record.classification == Classification::PolicyIncluded)
            .and_then(|record| record.adapter)
    }

    /// The discovered document at one exact raw path. Tree and index
    /// discovery both preserve the strict ordering documented on this type.
    #[must_use]
    pub(crate) fn document(&self, path: &[u8]) -> Option<&DocumentRecord> {
        self.documents
            .binary_search_by(|record| record.path.as_bytes().cmp(path))
            .ok()
            .and_then(|index| self.documents.get(index))
    }

    /// Whether a path is a scanned structured document on this side, which is
    /// what accepting a query requires.
    #[must_use]
    pub fn is_scanned_structured(&self, path: &RepoPath) -> bool {
        self.document(path.as_bytes()).is_some_and(|record| {
            record.classification != Classification::PlainAdvisory
                && matches!(record.status, DocumentStatus::Scanned(_))
        })
    }

    /// What this snapshot holds at `path`: an entry of its own, or a directory
    /// implied by the entries beneath it.
    #[must_use]
    pub fn locate(&self, path: &RepoPath) -> Option<Located<'_>> {
        if let Some((mode, oid)) = self.entries.get(path.as_bytes()) {
            return Some(Located::Entry(*mode, oid));
        }
        let mut under = path.as_bytes().to_vec();
        under.push(b'/');
        self.entries
            .range::<[u8], _>((
                std::ops::Bound::Included(under.as_slice()),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .filter(|(key, _)| key.as_bytes().starts_with(&under))
            .map(|_| Located::ImpliedTree)
    }
}

struct Frame {
    oid: Oid,
    prefix: Vec<u8>,
    entries: Vec<TreeEntry>,
    next: usize,
}

struct DocumentContext<'a> {
    repo: &'a Repository,
    includes: &'a Includes,
    scope: Option<&'a BTreeSet<RepoPath>>,
}

pub(crate) fn empty_discovery() -> SnapshotDiscovery {
    SnapshotDiscovery {
        documents: Vec::new(),
        labels: BTreeMap::new(),
        outside_document_set: 0,
        tree_entries: 0,
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
        published_routes: BTreeMap::new(),
        redirect_routes: BTreeMap::new(),
        sole_sites: BTreeMap::new(),
        sphinx_included: BTreeSet::new(),
        antora_components: BTreeMap::new(),
        source_suffixes: BTreeMap::new(),
        declared_routers: BTreeMap::new(),
        published_roots: BTreeMap::new(),
    }
}

fn charge_entry(discovery: &mut SnapshotDiscovery, limit: u64) -> Result<(), Error> {
    discovery.tree_entries = discovery.tree_entries.saturating_add(1);
    if discovery.tree_entries > limit {
        Err(crossing(
            ResourceName::GitTreeEntriesPerSnapshot,
            limit,
            limit.saturating_add(1),
        ))
    } else {
        Ok(())
    }
}

/// Every source file a Sphinx root reads under a suffix it declares, recorded
/// as the reStructuredText document that root reads it as. The declaration
/// sits beside documents the walk has already passed, so what it widens is
/// settled once the walk is over. Only the nearest `conf.py` above a path
/// answers for it, because a suffix a site declares says nothing about a file
/// outside that site, and a path the built-in rows or a policy include already
/// claim keeps the row it has.
fn declared_documents(
    context: &DocumentContext<'_>,
    git: &mut GitResources,
    scan: &mut ScanResources,
    discovery: &mut SnapshotDiscovery,
) -> Result<(), Error> {
    if discovery.source_suffixes.is_empty() {
        return Ok(());
    }
    let sources: Vec<(RepoPath, TreeEntry)> = discovery
        .entries
        .iter()
        .filter(|(path, (mode, _))| {
            matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile)
                && discovery.document(path.as_bytes()).is_none()
                && context
                    .scope
                    .is_none_or(|documents| documents.contains(*path))
                && crate::route::declared_source(discovery, path.as_bytes())
        })
        .map(|(path, (mode, oid))| {
            (
                path.clone(),
                TreeEntry {
                    mode: *mode,
                    name: path.as_bytes().to_vec(),
                    oid: oid.clone(),
                },
            )
        })
        .collect();
    for (path, entry) in sources {
        discovery.outside_document_set = discovery.outside_document_set.saturating_sub(1);
        record_document(
            context,
            git,
            scan,
            discovery,
            path,
            &entry,
            Some(Classification::StructuredRst),
        )?;
    }
    discovery
        .documents
        .sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(())
}

fn record_document(
    context: &DocumentContext<'_>,
    git: &mut GitResources,
    scan: &mut ScanResources,
    discovery: &mut SnapshotDiscovery,
    path: RepoPath,
    entry: &TreeEntry,
    declared: Option<Classification>,
) -> Result<(), Error> {
    let classification = match classify(path.as_bytes()).or(declared) {
        Some(native) => native,
        None if context.includes.matches(&path) => Classification::PolicyIncluded,
        None => {
            discovery.outside_document_set = discovery.outside_document_set.saturating_add(1);
            return Ok(());
        }
    };
    let adapter = if classification == Classification::PolicyIncluded {
        context.includes.binding(&path)
    } else {
        native_adapter(classification)
    };
    if context
        .scope
        .is_some_and(|documents| !documents.contains(&path))
    {
        return Ok(());
    }
    let (status, byte_count, raw_digest) = side_status(
        context.repo,
        git,
        scan,
        context.includes,
        adapter,
        &path,
        entry,
    )?;
    discovery.documents.push(DocumentRecord {
        path,
        classification,
        adapter,
        status,
        oid: entry.oid.clone(),
        mode: entry.mode,
        byte_count,
        raw_digest,
    });
    Ok(())
}

/// Vets one raw entry name under `prefix` and returns the admitted path:
/// text or bytes alike, refusing only the byte grammar. Length is charged on
/// the raw bytes first, so a refused name's disclosed bytes always fit the
/// report's frozen hex field, whose cap is the path ceiling itself; past the
/// ceiling the crossing row states both figures and carries no bytes.
fn admitted_path(
    defects: &mut Vec<PathDefect>,
    path_limit: u64,
    prefix: &[u8],
    name: &[u8],
) -> Option<RepoPath> {
    let mut raw = prefix.to_vec();
    if !raw.is_empty() {
        raw.push(b'/');
    }
    raw.extend_from_slice(name);
    let raw_bytes = u64::try_from(raw.len()).unwrap_or(u64::MAX);
    if raw_bytes > path_limit {
        defects.push(PathDefect {
            error: crossing(ResourceName::RawPathBytes, path_limit, raw_bytes),
            raw: None,
        });
        return None;
    }
    let admitted = RepoPath::from_bytes(raw.clone());
    if admitted.is_none() {
        defects.push(PathDefect {
            error: Error::UnrepresentablePath,
            raw: Some(raw),
        });
    }
    admitted
}

/// Walks one snapshot tree completely: iterative, expanding a shared subtree
/// at every distinct path, with a cycle only when a tree OID recurs on the
/// current ancestor stack. Each selected regular blob is admitted, read under
/// the document cap, checked for LFS pointer content, and scanned; symlink
/// and gitlink documents are unsupported sides; a defect scoped to one
/// document fails that document alone.
///
/// # Errors
///
/// A snapshot or evaluation budget crossing, an unreadable tree, or an
/// ancestor cycle ends discovery; everything narrower is recorded per path or
/// per document.
pub fn discover(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    includes: &Includes,
    root_tree: &Oid,
) -> Result<SnapshotDiscovery, Error> {
    discover_walk(
        repo,
        git,
        root_tree,
        WalkMode::Documents {
            scan,
            includes,
            scope: None,
        },
    )
}

/// Discovery restricted to an exact document set: the full tree walk, entry
/// budget, and path rules apply, but only scoped documents are acquired and
/// parsed. Debt adoption reproduction evaluates exactly its distinct debt
/// documents this way.
///
/// # Errors
///
/// Exactly as `discover`.
pub(crate) fn discover_scoped(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    includes: &Includes,
    root_tree: &Oid,
    scope: &BTreeSet<RepoPath>,
) -> Result<SnapshotDiscovery, Error> {
    discover_walk(
        repo,
        git,
        root_tree,
        WalkMode::Documents {
            scan,
            includes,
            scope: Some(scope),
        },
    )
}

pub(crate) enum WalkMode<'a> {
    Entries {
        selection: Option<(&'a [u8], &'a mut bool)>,
    },
    Documents {
        scan: &'a mut ScanResources,
        includes: &'a Includes,
        scope: Option<&'a BTreeSet<RepoPath>>,
    },
}

pub(crate) fn discover_walk(
    repo: &Repository,
    git: &mut GitResources,
    root_tree: &Oid,
    mut mode: WalkMode<'_>,
) -> Result<SnapshotDiscovery, Error> {
    let mut discovery = empty_discovery();
    let root = repo.read_expected(git, root_tree, ObjectKind::Tree)?;
    let mut frames = vec![Frame {
        oid: root_tree.clone(),
        prefix: Vec::new(),
        entries: parse_tree(repo.object_format(), &root.body)?,
        next: 0,
    }];

    while let Some(frame) = frames.last_mut() {
        let Some(entry) = frame.entries.get(frame.next).cloned() else {
            frames.pop();
            continue;
        };
        frame.next = frame.next.saturating_add(1);
        let prefix = frame.prefix.clone();

        charge_entry(&mut discovery, git.limits().tree_entries_per_snapshot)?;

        let Some(path) = admitted_path(
            &mut discovery.path_defects,
            git.limits().raw_path_bytes,
            &prefix,
            &entry.name,
        ) else {
            if let WalkMode::Entries {
                selection: Some((root, complete)),
            } = &mut mode
                && discovery.path_defects.last().is_some_and(|defect| {
                    let path = defect.raw.as_deref().unwrap_or(&prefix);
                    path == *root
                        || path
                            .strip_prefix(*root)
                            .is_some_and(|relative| relative.starts_with(b"/"))
                })
            {
                **complete = false;
            }
            continue;
        };

        discovery
            .entries
            .insert(path.clone(), (entry.mode, entry.oid.clone()));
        if entry.mode == GitMode::Tree {
            if frames.iter().any(|ancestor| ancestor.oid == entry.oid) {
                return Err(Error::Git(GitDefect::ObjectUnreadable));
            }
            let subtree = repo.read_expected(git, &entry.oid, ObjectKind::Tree)?;
            frames.push(Frame {
                oid: entry.oid.clone(),
                prefix: path.as_bytes().to_vec(),
                entries: parse_tree(repo.object_format(), &subtree.body)?,
                next: 0,
            });
            continue;
        }

        match &mut mode {
            WalkMode::Entries { .. } => {}
            WalkMode::Documents {
                scan,
                includes,
                scope,
            } => {
                let context = DocumentContext {
                    repo,
                    includes,
                    scope: *scope,
                };
                record_document(&context, git, scan, &mut discovery, path, &entry, None)?;
            }
        }
    }
    discovery.sole_sites = crate::route::sole_sites(&discovery);
    if let WalkMode::Documents {
        scan,
        includes,
        scope,
    } = &mut mode
    {
        let declared = descriptors(repo, git, scan, &discovery)?;
        discovery.antora_components = declared.antora_components;
        discovery.source_suffixes = declared.source_suffixes;
        discovery.declared_routers = declared.routers;
        discovery.published_roots = declared.published_roots;
        let context = DocumentContext {
            repo,
            includes,
            scope: *scope,
        };
        declared_documents(&context, git, scan, &mut discovery)?;
    }
    (discovery.published_routes, discovery.redirect_routes) =
        crate::route::published_routes(&discovery);
    if let WalkMode::Documents { scan, .. } = &mut mode {
        settle_roles(scan, &mut discovery)?;
    }
    Ok(discovery)
}

/// Discovery over the complete logical stage-zero index: the synthetic
/// candidate's entries take the place of a tree walk, under the same entry
/// budget, path rules, classification, and side outcomes. Blob and symlink
/// rows must name objects present in the primary database.
///
/// # Errors
///
/// Everything tree discovery fails with, plus `ObjectMissing` for an index
/// row whose object is absent.
pub fn discover_index(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    includes: &Includes,
    index: &amiss_git::LogicalIndex,
) -> Result<SnapshotDiscovery, Error> {
    let context = DocumentContext {
        repo,
        includes,
        scope: None,
    };
    let mut discovery = empty_discovery();
    for entry in &index.entries {
        charge_entry(&mut discovery, git.limits().tree_entries_per_snapshot)?;
        let Some(path) = admitted_path(
            &mut discovery.path_defects,
            git.limits().raw_path_bytes,
            &[],
            &entry.path,
        ) else {
            continue;
        };
        if entry.mode != GitMode::Gitlink && !repo.has_object(git, &entry.oid)? {
            return Err(Error::Git(GitDefect::ObjectMissing));
        }

        discovery
            .entries
            .insert(path.clone(), (entry.mode, entry.oid.clone()));
        let tree_entry = TreeEntry {
            mode: entry.mode,
            name: entry.path.clone(),
            oid: entry.oid.clone(),
        };
        record_document(&context, git, scan, &mut discovery, path, &tree_entry, None)?;
    }
    discovery.sole_sites = crate::route::sole_sites(&discovery);
    let declared = descriptors(repo, git, scan, &discovery)?;
    discovery.antora_components = declared.antora_components;
    discovery.source_suffixes = declared.source_suffixes;
    discovery.declared_routers = declared.routers;
    discovery.published_roots = declared.published_roots;
    declared_documents(&context, git, scan, &mut discovery)?;
    (discovery.published_routes, discovery.redirect_routes) =
        crate::route::published_routes(&discovery);
    settle_roles(scan, &mut discovery)?;
    Ok(discovery)
}

/// What one defect makes of the document it is scoped to. Bytes that will not
/// decode and the per-document ceilings are facts about that one file: the
/// document becomes a boundary the report counts with its reason, and the run
/// keeps its other documents. A parser that broke its own contract, an object
/// the store cannot produce, and every run-wide budget say nothing about the
/// file, so they still fail.
fn document_outcome(defect: Error) -> Result<DocumentStatus, Error> {
    if !defect.is_document_scoped() {
        return Err(defect);
    }
    match defect {
        Error::Parse(Fault::DocumentInvalid) => {
            Ok(DocumentStatus::Unsupported(UnsupportedKind::Undecodable))
        }
        Error::ResourceLimit { .. } => Ok(DocumentStatus::Unsupported(UnsupportedKind::Ceiling)),
        Error::Parse(Fault::ParserError | Fault::ParserPanic | Fault::InvalidSourceSpan)
        | Error::Git(_)
        | Error::UnrepresentablePath
        | Error::Internal => Ok(DocumentStatus::Failed(defect)),
    }
}

/// One selected non-tree entry's outcome. Exclusion is decided before any
/// read, a symlink or gitlink is never read, and a regular blob is admitted,
/// read under the document cap, then recognized as pointer content or
/// scanned.
fn side_status(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    includes: &Includes,
    adapter: Option<Adapter>,
    path: &RepoPath,
    entry: &TreeEntry,
) -> Result<(DocumentStatus, u64, Option<amiss_wire::model::Digest>), Error> {
    if excluded_by_built_in(path.as_bytes()) && !includes.matches(path) {
        return Ok((DocumentStatus::ExcludedBuiltIn, 0, None));
    }
    match entry.mode {
        GitMode::Symlink => {
            return Ok((
                DocumentStatus::Unsupported(UnsupportedKind::Symlink),
                0,
                None,
            ));
        }
        GitMode::Gitlink => {
            return Ok((
                DocumentStatus::Unsupported(UnsupportedKind::Gitlink),
                0,
                None,
            ));
        }
        GitMode::Tree => return Err(Error::Git(GitDefect::ObjectUnreadable)),
        GitMode::RegularFile | GitMode::ExecutableFile => {}
    }

    scan.admit_document()?;
    let identity = adapter.map(|adapter| ScanIdentity {
        oid: entry.oid.clone(),
        adapter,
        embedded_code_allowance: (adapter == Adapter::Mdx).then(|| scan.embedded_code_allowance()),
    });
    let memo = identity
        .as_ref()
        .and_then(|identity| scan.scans.get(identity))
        .cloned();
    let (scanned, byte_count, raw) = if let Some(memo) = memo {
        // A blob the other side scanned is not read again: its bytes are its oid's.
        scan.charge_document_bytes(memo.byte_count)?;
        let replayed = replay_scan_charges(scan, &memo.scanned).map(|()| memo.scanned);
        (replayed, memo.byte_count, memo.raw_digest)
    } else {
        let cap = ValueCap {
            resource: ResourceName::DocumentBlobBytes,
            limit: scan.limits().document_blob_bytes,
        };
        let object = match repo.read_expected_capped(git, &entry.oid, ObjectKind::Blob, cap) {
            Ok(object) => object,
            Err(defect) => {
                return document_outcome(Error::from(defect)).map(|status| (status, 0, None));
            }
        };
        let byte_count = u64::try_from(object.body.len()).unwrap_or(u64::MAX);
        scan.charge_document_bytes(byte_count)?;
        let raw = amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(crate::resolve::RAW_EVIDENCE_DOMAIN)
                .chain_update([0_u8])
                .chain_update(&object.body)
                .finalize()
                .0,
        );
        if lfs::is_pointer(&object.body) {
            return Ok((
                DocumentStatus::Unsupported(UnsupportedKind::LfsPointer),
                byte_count,
                Some(raw),
            ));
        }
        let Some(identity) = identity else {
            return Ok((
                DocumentStatus::Unsupported(UnsupportedKind::Format),
                byte_count,
                Some(raw),
            ));
        };
        let scanned = scan_bytes(scan, identity.adapter, &object.body)
            .map(Arc::new)
            .inspect(|scanned| {
                scan.scans.insert(
                    identity,
                    ScanMemo {
                        scanned: Arc::clone(scanned),
                        byte_count,
                        raw_digest: raw,
                    },
                );
            });
        (scanned, byte_count, raw)
    };
    match scanned {
        Ok(scanned) => Ok((DocumentStatus::Scanned(scanned), byte_count, Some(raw))),
        Err(defect) => document_outcome(defect).map(|status| (status, byte_count, Some(raw))),
    }
}
