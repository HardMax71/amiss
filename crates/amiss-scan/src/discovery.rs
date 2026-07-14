use std::collections::BTreeMap;

use amiss_git::{GitResources, ObjectKind, Repository, TreeEntry, ValueCap, parse_tree};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::model::{Oid, RepoPath};

use crate::document::{Classification, classify, excluded_by_built_in};
use crate::policy::Includes;
use crate::resources::{ScanResources, crossing};
use crate::scan::{Scanned, scan_bytes};
use crate::{Error, GitDefect, lfs};

/// The deliberate object and format boundaries a discovered document side can
/// sit behind without failing the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedKind {
    Symlink,
    Gitlink,
    LfsPointer,
    Format,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Scanned(Scanned),
    ExcludedBuiltIn,
    Unsupported(UnsupportedKind),
    Failed(Error),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRecord {
    pub path: String,
    pub classification: Classification,
    pub status: DocumentStatus,
    pub oid: Oid,
    pub mode: GitMode,
    pub byte_count: u64,
    pub raw_digest: Option<amiss_wire::digest::Digest>,
}

/// One side's complete discovery: every classified path in repository byte
/// order with its outcome, the count of non-tree entries outside the document
/// set, the entries walked, and the path-level defects that never became a
/// document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDiscovery {
    pub documents: Vec<DocumentRecord>,
    pub outside_document_set: u64,
    pub tree_entries: u64,
    pub path_defects: Vec<Error>,
    pub entries: BTreeMap<String, (GitMode, Oid)>,
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
    /// Whether a path is a scanned structured document on this side, which is
    /// what accepting a query requires.
    #[must_use]
    pub fn is_scanned_structured(&self, path: &str) -> bool {
        self.documents.iter().any(|record| {
            record.path == path
                && record.classification != Classification::PlainAdvisory
                && matches!(record.status, DocumentStatus::Scanned(_))
        })
    }

    /// What this snapshot holds at `path`: an entry of its own, or a directory
    /// implied by the entries beneath it.
    #[must_use]
    pub fn locate(&self, path: &str) -> Option<Located<'_>> {
        if let Some((mode, oid)) = self.entries.get(path) {
            return Some(Located::Entry(*mode, oid));
        }
        let under = format!("{path}/");
        self.entries
            .range(under.clone()..)
            .next()
            .filter(|(key, _)| key.starts_with(&under))
            .map(|_| Located::ImpliedTree)
    }
}

struct Frame {
    oid: Oid,
    prefix: String,
    entries: Vec<TreeEntry>,
    next: usize,
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
    discover_walk(repo, git, scan, includes, root_tree, None)
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
    scope: &std::collections::BTreeSet<String>,
) -> Result<SnapshotDiscovery, Error> {
    discover_walk(repo, git, scan, includes, root_tree, Some(scope))
}

fn discover_walk(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    includes: &Includes,
    root_tree: &Oid,
    scope: Option<&std::collections::BTreeSet<String>>,
) -> Result<SnapshotDiscovery, Error> {
    let mut discovery = SnapshotDiscovery {
        documents: Vec::new(),
        outside_document_set: 0,
        tree_entries: 0,
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
    };
    let root = repo.read_expected(git, root_tree, ObjectKind::Tree)?;
    let mut frames = vec![Frame {
        oid: root_tree.clone(),
        prefix: String::new(),
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

        discovery.tree_entries = discovery.tree_entries.saturating_add(1);
        let entry_limit = git.limits().tree_entries_per_snapshot;
        if discovery.tree_entries > entry_limit {
            return Err(crossing(
                ResourceName::GitTreeEntriesPerSnapshot,
                entry_limit,
                entry_limit.saturating_add(1),
            ));
        }

        let Ok(name) = str::from_utf8(&entry.name) else {
            discovery.path_defects.push(Error::UnrepresentablePath);
            continue;
        };
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let path_limit = git.limits().raw_path_bytes;
        let path_bytes = u64::try_from(path.len()).unwrap_or(u64::MAX);
        if path_bytes > path_limit {
            discovery.path_defects.push(crossing(
                ResourceName::RawPathBytes,
                path_limit,
                path_bytes,
            ));
            continue;
        }
        if RepoPath::new(path.clone()).is_none() {
            discovery.path_defects.push(Error::UnrepresentablePath);
            continue;
        }

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
                prefix: path,
                entries: parse_tree(repo.object_format(), &subtree.body)?,
                next: 0,
            });
            continue;
        }

        let classification = match classify(&path) {
            Some(native) => native,
            None if includes.matches(&path) => Classification::PolicyIncluded,
            None => {
                discovery.outside_document_set = discovery.outside_document_set.saturating_add(1);
                continue;
            }
        };
        if scope.is_some_and(|documents| !documents.contains(&path)) {
            continue;
        }
        let (status, byte_count, raw_digest) =
            side_status(repo, git, scan, includes, classification, &path, &entry)?;
        discovery.documents.push(DocumentRecord {
            path,
            classification,
            status,
            oid: entry.oid.clone(),
            mode: entry.mode,
            byte_count,
            raw_digest,
        });
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
    let mut discovery = SnapshotDiscovery {
        documents: Vec::new(),
        outside_document_set: 0,
        tree_entries: 0,
        path_defects: Vec::new(),
        entries: BTreeMap::new(),
    };
    for entry in &index.entries {
        discovery.tree_entries = discovery.tree_entries.saturating_add(1);
        let entry_limit = git.limits().tree_entries_per_snapshot;
        if discovery.tree_entries > entry_limit {
            return Err(crossing(
                ResourceName::GitTreeEntriesPerSnapshot,
                entry_limit,
                entry_limit.saturating_add(1),
            ));
        }
        let Ok(path) = str::from_utf8(&entry.path) else {
            discovery.path_defects.push(Error::UnrepresentablePath);
            continue;
        };
        let path_limit = git.limits().raw_path_bytes;
        let path_bytes = u64::try_from(path.len()).unwrap_or(u64::MAX);
        if path_bytes > path_limit {
            discovery.path_defects.push(crossing(
                ResourceName::RawPathBytes,
                path_limit,
                path_bytes,
            ));
            continue;
        }
        if RepoPath::new(path.to_owned()).is_none() {
            discovery.path_defects.push(Error::UnrepresentablePath);
            continue;
        }
        if entry.mode != GitMode::Gitlink && !repo.has_object(git, &entry.oid)? {
            return Err(Error::Git(GitDefect::ObjectMissing));
        }

        discovery
            .entries
            .insert(path.to_owned(), (entry.mode, entry.oid.clone()));
        let classification = match classify(path) {
            Some(native) => native,
            None if includes.matches(path) => Classification::PolicyIncluded,
            None => {
                discovery.outside_document_set = discovery.outside_document_set.saturating_add(1);
                continue;
            }
        };
        let tree_entry = TreeEntry {
            mode: entry.mode,
            name: entry.path.clone(),
            oid: entry.oid.clone(),
        };
        let (status, byte_count, raw_digest) =
            side_status(repo, git, scan, includes, classification, path, &tree_entry)?;
        discovery.documents.push(DocumentRecord {
            path: path.to_owned(),
            classification,
            status,
            oid: entry.oid.clone(),
            mode: entry.mode,
            byte_count,
            raw_digest,
        });
    }
    Ok(discovery)
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
    classification: Classification,
    path: &str,
    entry: &TreeEntry,
) -> Result<(DocumentStatus, u64, Option<amiss_wire::digest::Digest>), Error> {
    if excluded_by_built_in(path) && !includes.matches(path) {
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
    let cap = ValueCap {
        resource: ResourceName::DocumentBlobBytes,
        limit: scan.limits().document_blob_bytes,
    };
    let object = match repo.read_expected_capped(git, &entry.oid, ObjectKind::Blob, cap) {
        Ok(object) => object,
        Err(defect) => {
            let defect = Error::from(defect);
            if defect.is_document_scoped() {
                return Ok((DocumentStatus::Failed(defect), 0, None));
            }
            return Err(defect);
        }
    };
    let byte_count = u64::try_from(object.body.len()).unwrap_or(u64::MAX);
    scan.charge_document_bytes(byte_count)?;
    let raw = amiss_wire::digest::hb(crate::resolve::RAW_EVIDENCE_DOMAIN, &object.body);
    if lfs::is_pointer(&object.body) {
        return Ok((
            DocumentStatus::Unsupported(UnsupportedKind::LfsPointer),
            byte_count,
            Some(raw),
        ));
    }
    let Some(adapter) = classification.adapter() else {
        return Ok((
            DocumentStatus::Unsupported(UnsupportedKind::Format),
            byte_count,
            Some(raw),
        ));
    };
    match scan_bytes(scan, adapter, &object.body) {
        Ok(scanned) => Ok((DocumentStatus::Scanned(scanned), byte_count, Some(raw))),
        Err(defect) if defect.is_document_scoped() => {
            Ok((DocumentStatus::Failed(defect), byte_count, Some(raw)))
        }
        Err(defect) => Err(defect),
    }
}
