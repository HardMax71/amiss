use sha2::Digest as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_git::{GitResources, ObjectKind, Repository, TreeEntry, ValueCap, parse_tree};
use amiss_md::Fault;
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::extraction::SourceConstruct;
use amiss_wire::model::{Adapter, Oid, RepoPath};

use crate::document::{DocumentClassification, classify, excluded_by_built_in, native_adapter};
use crate::policy::Includes;
use crate::resources::{ScanIdentity, ScanMemo, ScanResources, crossing};
use crate::route::DIRECTORY_PAGES;
use crate::route::DOCUSAURUS;
use crate::route::ROUTER_DECLARATION;
use crate::route::ROUTERS;
use crate::route::RouteRule;
use crate::route::SHARED_CONFIGS;
use crate::route::SPHINX;
use crate::route::Spelling;
use crate::route::ancestor_root;
use crate::route::content_root;
use crate::route::declarable;
use crate::route::declaring_directory;
use crate::route::directory;
use crate::route::join;
use crate::route::normalized_native_path;
use crate::route::normalized_path_under;
use crate::route::page_route;
use crate::route::sole_claims;
use crate::scan::{replay_scan_charges, scan_bytes};
use crate::scanned::Scanned;
use crate::scanned::ScannedOccurrence;
use crate::{Error, GitDefect, lfs};
use amiss_wire::controls::TargetKind;
use amiss_wire::extraction::Transclusion;
use amiss_wire::extraction::TransclusionKind;
use amiss_wire::extraction::TransclusionRefusal;
use amiss_wire::uri::scheme;

/// The deliberate object and format boundaries a discovered document side can
/// sit behind without failing the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedKind {
    Symlink,
    Gitlink,
    LfsPointer,
    Format,
    Undecodable,
    Unparsable,
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
    pub classification: DocumentClassification,
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
            record.adapter == Some(Adapter::Markdown) && sphinx_governed(discovery, &record.path)
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
        record_declaration(discovery, &mut declared, path, &object.body);
    }
    Ok(declared)
}

/// A scan the MDX grammar refused, tried again with each HTML comment it
/// rejected read as a comment, which is how a Docusaurus site reads it. The
/// result is marked, since only a site that reads comments keeps it once the
/// sites are known. Any other defect, and a source still refused, stands.
fn commented(
    scan: &mut ScanResources,
    adapter: Adapter,
    source: &[u8],
    defect: Error,
) -> Result<Scanned, Error> {
    let refused = adapter == Adapter::Mdx && defect == Error::Parse(Fault::DocumentUnparsable);
    let Some(read) = refused.then(|| amiss_md::comments_read(source)).flatten() else {
        return Err(defect);
    };
    let mut scanned = scan_bytes(scan, adapter, &read)?;
    scanned.commented = true;
    Ok(scanned)
}

/// A document only its HTML comments made readable stays read where a
/// Docusaurus site holds it and is refused anywhere else. The walk that read
/// it could not yet ask which site holds it.
fn settle_comments(discovery: &mut SnapshotDiscovery) {
    let refused: Vec<usize> = discovery
        .documents
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            matches!(&record.status, DocumentStatus::Scanned(scanned) if scanned.commented)
                && site_root(discovery, record.path.as_bytes(), &DOCUSAURUS).is_none()
        })
        .map(|(index, _)| index)
        .collect();
    for index in refused {
        if let Some(record) = discovery.documents.get_mut(index) {
            record.status = DocumentStatus::Unsupported(UnsupportedKind::Unparsable);
        }
    }
}

/// Whether a path is one of the five files read for what it declares. A
/// Sphinx configuration under a tree the scan excludes belongs to a fixture
/// rather than to a site the repository publishes, the same reading that keeps
/// Sphinx's own 176 test roots from declaring anything.
fn declaring(path: &RepoPath) -> bool {
    let router = path
        .as_bytes()
        .strip_suffix(ROUTER_DECLARATION.as_bytes())
        .is_some_and(|above| above.is_empty() || above.ends_with(b"/"));
    router
        || crate::route::declares(&crate::route::ANTORA, path)
        || configures(path)
        || publishes(path)
        || binds_book(path)
        || crate::route::declares(&crate::route::ASTRO, path)
}

fn configures(path: &RepoPath) -> bool {
    crate::route::declares(&SPHINX, path) && !excluded_by_built_in(path.as_bytes())
}

fn publishes(path: &RepoPath) -> bool {
    crate::route::declares(&crate::route::HUGO, path) && !excluded_by_built_in(path.as_bytes())
}

fn binds_book(path: &RepoPath) -> bool {
    crate::route::declares(&crate::route::MDBOOK_PAGES, path)
        && !excluded_by_built_in(path.as_bytes())
}

/// The package an Astro configuration imports Starlight from.
const STARLIGHT: &[u8] = b"@astrojs/starlight";

/// What one descriptor's own bytes say, under the reading its name selects.
fn record_declaration(
    discovery: &SnapshotDiscovery,
    declared: &mut Declared,
    path: RepoPath,
    body: &[u8],
) {
    if crate::route::declares(&crate::route::ANTORA, &path) {
        if let Some(component) = crate::route::antora_descriptor(body) {
            declared.antora_components.insert(path, component);
        }
    } else if configures(&path) {
        let suffixes = crate::route::source_suffixes(body);
        if !suffixes.is_empty() {
            declared
                .source_suffixes
                .insert(directory(path.as_bytes()).to_vec(), suffixes);
        }
    } else if publishes(&path) {
        record_publication(discovery, declared, path, body);
    } else if crate::route::declares(&crate::route::ASTRO, &path) {
        if body
            .windows(STARLIGHT.len())
            .any(|window| window == STARLIGHT)
        {
            declared
                .starlight_roots
                .insert(directory(path.as_bytes()).to_vec());
        }
    } else if binds_book(&path) {
        let root = directory(path.as_bytes());
        if let Some(source) = crate::route::book_source(root, body) {
            declared.book_sources.insert(root.to_vec(), source);
        }
    } else if let Some(router) = crate::route::declared_router(body) {
        declared.routers.insert(path, router);
    }
}

/// What one generator configuration says. A name several generators share
/// belongs to the rule its bindings select, and to none where they select
/// nothing. Hugo's rule withholds answers rather than adding them, so under a
/// shared name it also needs the content directory the file names to sit
/// beside it, the way Zola's reading needs its own. A Hugo configuration names
/// the content roots of the project it declares, which is the directory its
/// name is read against.
fn record_publication(
    discovery: &SnapshotDiscovery,
    declared: &mut Declared,
    path: RepoPath,
    body: &[u8],
) {
    let Some(name) = crate::route::HUGO
        .declared_by
        .iter()
        .filter(|name| declaring_directory(path.as_bytes(), name).is_some())
        .max_by_key(|name| name.len())
    else {
        return;
    };
    let shared = SHARED_CONFIGS.contains(name);
    let rule = if shared {
        crate::route::addressed_rule(body)
    } else {
        Some(&crate::route::HUGO)
    };
    let Some(rule) = rule else {
        return;
    };
    let project = declaring_directory(path.as_bytes(), name)
        .unwrap_or_default()
        .to_vec();
    let roots: Vec<(Vec<u8>, String)> = crate::route::hugo_project(body)
        .into_iter()
        .map(|(root, base)| (join(&project, root.as_bytes()), base))
        .collect();
    let hugo = rule.name == crate::route::HUGO.name;
    let content = roots.iter().any(|(root, _)| {
        RepoPath::from_bytes(root.clone()).is_some_and(|root| {
            matches!(
                discovery.locate(&root),
                Some(Located::ImpliedTree | Located::Entry(GitMode::Tree, _))
            )
        })
    });
    if shared && hugo && !content {
        return;
    }
    if shared {
        declared.bound_configs.insert(path, rule.declared_by);
    }
    if hugo {
        declared.published_roots.insert(project, roots);
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
    bound_configs: BTreeMap<RepoPath, &'static [&'static str]>,
    book_sources: BTreeMap<Vec<u8>, Vec<u8>>,
    starlight_roots: BTreeSet<Vec<u8>>,
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
        .filter(|record| markdown(record) && sphinx_governed(discovery, &record.path))
        .map(|record| record.path.clone())
        .collect();
    let mut found = BTreeSet::new();
    while let Some(document) = frontier.pop() {
        for target in included_documents(discovery, &document) {
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
    /// The names of the rule each configuration under a shared name selects.
    pub bound_configs: BTreeMap<RepoPath, &'static [&'static str]>,
    /// The directory each mdBook reads its chapters from, by the directory
    /// holding its `book.toml`.
    pub book_sources: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Each Astro project whose configuration loads Starlight, by the
    /// directory holding that configuration.
    pub starlight_roots: BTreeSet<Vec<u8>>,
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
            .filter(|record| record.classification == DocumentClassification::PolicyIncluded)
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
            record.classification != DocumentClassification::PlainAdvisory
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
        bound_configs: BTreeMap::new(),
        book_sources: BTreeMap::new(),
        starlight_roots: BTreeSet::new(),
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
                && declared_source(discovery, path.as_bytes())
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
            Some(DocumentClassification::StructuredRst),
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
    declared: Option<DocumentClassification>,
) -> Result<(), Error> {
    let classification = match classify(path.as_bytes()).or(declared) {
        Some(native) => native,
        None if context.includes.matches(&path) => DocumentClassification::PolicyIncluded,
        None => {
            discovery.outside_document_set = discovery.outside_document_set.saturating_add(1);
            return Ok(());
        }
    };
    let adapter = if classification == DocumentClassification::PolicyIncluded {
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
    discovery.sole_sites = sole_sites(&discovery);
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
        discovery.bound_configs = declared.bound_configs;
        discovery.book_sources = declared.book_sources;
        discovery.starlight_roots = declared.starlight_roots;
        let context = DocumentContext {
            repo,
            includes,
            scope: *scope,
        };
        declared_documents(&context, git, scan, &mut discovery)?;
        settle_comments(&mut discovery);
    }
    (discovery.published_routes, discovery.redirect_routes) = published_routes(&discovery);
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
    discovery.sole_sites = sole_sites(&discovery);
    let declared = descriptors(repo, git, scan, &discovery)?;
    discovery.antora_components = declared.antora_components;
    discovery.source_suffixes = declared.source_suffixes;
    discovery.declared_routers = declared.routers;
    discovery.published_roots = declared.published_roots;
    discovery.bound_configs = declared.bound_configs;
    discovery.book_sources = declared.book_sources;
    discovery.starlight_roots = declared.starlight_roots;
    declared_documents(&context, git, scan, &mut discovery)?;
    settle_comments(&mut discovery);
    (discovery.published_routes, discovery.redirect_routes) = published_routes(&discovery);
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
        Error::Parse(Fault::DocumentUnparsable) => {
            Ok(DocumentStatus::Unsupported(UnsupportedKind::Unparsable))
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
            sha2::Sha256::new_with_prefix(amiss_wire::model::RAW_EVIDENCE_DOMAIN)
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
            .or_else(|defect| commented(scan, identity.adapter, &object.body, defect))
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

/// Whether Sphinx parses this document, which is what turns the `MyST`
/// spellings on. A declaration above the file is one way, and the route table
/// reads the same file to anchor a source-root docname, so the answer is taken
/// from there rather than spelled twice. A page of the tree including the file
/// is the other way, and that one reaches outside the declared root.
pub(crate) fn sphinx_governed(snapshot: &SnapshotDiscovery, document: &RepoPath) -> bool {
    ROUTERS
        .iter()
        .filter(|rule| rule.serves(Spelling::SourceRoot))
        .any(|rule| declared_root(snapshot, document.as_bytes(), rule.declared_by).is_some())
        || snapshot.sphinx_included.contains(document)
}

/// The includes an expansion walks. A call a template answers is no edge at
/// all, since nothing in the tree stands in for what it writes, so `templated`
/// answers it instead and the walk passes it by.
pub(crate) fn followed(transclusions: &[Transclusion]) -> Vec<&Transclusion> {
    transclusions
        .iter()
        .filter(|entry| {
            !matches!(
                entry.kind,
                Err(TransclusionRefusal::Template | TransclusionRefusal::Liquid)
            )
        })
        .collect()
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

pub(crate) fn local_target(
    root: Option<&[u8]>,
    document: &RepoPath,
    target: &str,
) -> Option<RepoPath> {
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
pub(crate) fn snippet_root(
    snapshot: &SnapshotDiscovery,
    adapter: Adapter,
    document: &RepoPath,
) -> Option<Vec<u8>> {
    if adapter != Adapter::Markdown {
        return None;
    }
    declared_root(
        snapshot,
        document.as_bytes(),
        crate::route::MKDOCS.declared_by,
    )
}

/// Whether a Sphinx root reads this path as one of its own source files,
/// which is the nearest `conf.py` above it declaring the suffix it carries.
#[must_use]
fn declared_source(snapshot: &SnapshotDiscovery, path: &[u8]) -> bool {
    let Some(root) = declared_root(snapshot, path, SPHINX.declared_by) else {
        return false;
    };
    snapshot.source_suffixes.get(&root).is_some_and(|declared| {
        declared
            .iter()
            .any(|suffix| path.ends_with(suffix.as_bytes()))
    })
}

/// Every route the snapshot's documents claim and the document claiming each,
/// as the routes pages are published at and, apart from them, the page URLs
/// pages declare they moved away from. The two stay apart because a published
/// route answers a path as well as a URL while a redirect answers only a URL.
#[must_use]
pub(crate) fn published_routes(
    snapshot: &SnapshotDiscovery,
) -> (BTreeMap<RepoPath, RepoPath>, BTreeMap<RepoPath, RepoPath>) {
    let names = published_by(snapshot, &DOCUSAURUS);
    let redirects = published_by(snapshot, &DIRECTORY_PAGES);
    let mut published: Vec<(RepoPath, RepoPath)> = Vec::new();
    let mut moved: Vec<(RepoPath, RepoPath)> = Vec::new();
    if !names && !redirects {
        return (sole_claims(published), sole_claims(moved));
    }
    for record in &snapshot.documents {
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        if !matches!(record.adapter, Some(Adapter::Markdown | Adapter::Mdx)) {
            continue;
        }
        if names
            && let Some(route) =
                published_route(snapshot, &record.path, scanned.declared_name.as_deref())
        {
            published.push((route, record.path.clone()));
        }
        if redirects {
            moved.extend(
                moved_from(snapshot, &record.path, &scanned.declared_redirects)
                    .into_iter()
                    .map(|route| (route, record.path.clone())),
            );
        }
    }
    (sole_claims(published), sole_claims(moved))
}

/// Every page URL one document declares it moved away from. The block holds
/// what a browser would ask for, so each entry is read against the directory
/// the page's own URL sits in, which is one level above the route the page is
/// published at. An entry opening with a slash names a site route, and stays
/// where every site route stays. A router that serves no page URL above this
/// document leaves the block meaning nothing.
fn moved_from(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    declared: &[String],
) -> Vec<RepoPath> {
    if declared.is_empty() || site_root(snapshot, document.as_bytes(), &DIRECTORY_PAGES).is_none() {
        return Vec::new();
    }
    let published = page_route(document.as_bytes(), true);
    let parent = directory(&published).to_vec();
    declared
        .iter()
        .filter(|entry| !entry.starts_with('/'))
        .filter_map(|entry| normalized_path_under(&parent, false, entry).ok())
        .map(|(route, _kind)| route)
        .collect()
}

/// Where one document is published: the name it declares in place of its own
/// file name, that name under the content root it sits in when it opens with
/// a slash, and the route its own path spells when it declares nothing.
fn published_route(
    snapshot: &SnapshotDiscovery,
    document: &RepoPath,
    declared: Option<&str>,
) -> Option<RepoPath> {
    let raw = document.as_bytes();
    let Some(name) = declared else {
        return RepoPath::from_bytes(page_route(raw, false));
    };
    let Some(absolute) = name.strip_prefix('/') else {
        return RepoPath::from_bytes(join(directory(raw), name.as_bytes()));
    };
    let site = declared_root(snapshot, raw, DOCUSAURUS.declared_by)?;
    let root = content_root(&site, raw).unwrap_or(site);
    RepoPath::from_bytes(join(&root, absolute.as_bytes()))
}

/// Every directory this tree declares one rule's generator in. A declaration
/// under a tree the scan excludes belongs to a fixture or a dependency rather
/// than to a site the repository publishes, so it names no directory here.
fn declaring_directories(snapshot: &SnapshotDiscovery, rule: &RouteRule) -> BTreeSet<Vec<u8>> {
    snapshot
        .entries
        .iter()
        .filter(|(path, (mode, _))| {
            matches!(mode, GitMode::RegularFile | GitMode::ExecutableFile)
                && !excluded_by_built_in(path.as_bytes())
        })
        .filter_map(|(path, _)| {
            let name = rule
                .declared_by
                .iter()
                .filter(|name| declaring_directory(path.as_bytes(), name).is_some())
                .max_by_key(|name| name.len())?;
            let bound = !SHARED_CONFIGS.contains(name)
                || snapshot
                    .bound_configs
                    .get(path)
                    .is_some_and(|owner| *owner == rule.declared_by);
            declaring_directory(path.as_bytes(), name)
                .filter(|_| bound)
                .map(<[u8]>::to_vec)
        })
        .collect()
}

/// Whether the routes one rule serves are read for this tree at all: a
/// configuration file selecting the rule somewhere in the tree, or a
/// declaration naming it.
fn published_by(snapshot: &SnapshotDiscovery, rule: &RouteRule) -> bool {
    !declaring_directories(snapshot, rule).is_empty()
        || snapshot
            .declared_routers
            .values()
            .any(|(declared, _)| declared == rule.name)
}

/// The directory each generator this tree declares exactly once is configured
/// in. Which sites a tree declares is a question about the whole tree, since
/// a site under `website/` commonly reads `../docs` and the configuration
/// naming that directory is one this engine does not read. A generator
/// declared in several places fixes no owner for a document outside them all.
#[must_use]
pub(crate) fn sole_sites(snapshot: &SnapshotDiscovery) -> BTreeMap<&'static str, Vec<u8>> {
    ROUTERS
        .iter()
        .filter_map(|rule| {
            let mut declaring = declaring_directories(snapshot, rule).into_iter();
            let root = declaring.next()?;
            declaring.next().is_none().then_some((rule.name, root))
        })
        .collect()
}

/// Which site owns a document: the nearest configuration above it, the router
/// the repository declares for the directory, and failing both the single
/// site the tree declares. A rule serving a built page or a built route
/// answers for the build rather than for the tree, so widening one would
/// withhold an answer for a document no site publishes, and those keep the
/// ancestor walk. The rest can only reach a file the tree already holds.
pub fn site_root(snapshot: &SnapshotDiscovery, path: &[u8], rule: &RouteRule) -> Option<Vec<u8>> {
    let widens = !rule.serves(Spelling::BuiltRoute) && !rule.serves(Spelling::BuiltPage);
    let tree = snapshot.sole_sites.get(rule.name).filter(|_| widens);
    declared_root(snapshot, path, rule.declared_by)
        .or_else(|| declared_site(snapshot, path, rule))
        .or_else(|| tree.cloned())
}

/// The nearest directory above the document whose own declaration names this
/// rule's router. A repository may only name a rule whose every spelling
/// resolves a destination against the tree, so what it declares widens what
/// resolves and withholds no answer.
fn declared_site(
    snapshot: &SnapshotDiscovery,
    document: &[u8],
    rule: &RouteRule,
) -> Option<Vec<u8>> {
    if !declarable(rule) {
        return None;
    }
    let root = ancestor_root(document, &|directory| {
        declared_at(snapshot, directory).is_some()
    })?;
    (declared_at(snapshot, &root)?.0 == rule.name).then_some(root)
}

/// What a declaration sitting in this exact directory names. The declarations
/// a comparison reads are the candidate's on both sides, so this asks what
/// was declared rather than which tree holds the file.
pub(crate) fn declared_at<'snapshot>(
    snapshot: &'snapshot SnapshotDiscovery,
    directory: &[u8],
) -> Option<&'snapshot (String, Option<String>)> {
    RepoPath::from_bytes(join(directory, ROUTER_DECLARATION.as_bytes()))
        .and_then(|path| snapshot.declared_routers.get(&path))
}

/// The nearest directory on the document's ancestor chain holding one of the
/// named files, which is where a generator's own configuration selects its
/// rule and where a repository's own declaration sits.
pub(crate) fn declared_root(
    snapshot: &SnapshotDiscovery,
    document: &[u8],
    declared_by: &[&str],
) -> Option<Vec<u8>> {
    ancestor_root(document, &|directory| {
        declared_by.iter().any(|name| {
            let path = join(directory, name.as_bytes());
            let bound = !SHARED_CONFIGS.contains(name)
                || RepoPath::from_bytes(path.clone())
                    .and_then(|path| snapshot.bound_configs.get(&path))
                    .is_some_and(|owner| *owner == declared_by);
            bound && regular_file(snapshot, path)
        })
    })
}

pub(crate) fn regular_file(snapshot: &SnapshotDiscovery, path: Vec<u8>) -> bool {
    RepoPath::from_bytes(path).is_some_and(|path| {
        matches!(
            snapshot.locate(&path),
            Some(Located::Entry(
                GitMode::RegularFile | GitMode::ExecutableFile,
                _
            ))
        )
    })
}
