use amiss_git::{GitResources, ObjectKind, Repository, TreeEntry, ValueCap, parse_tree};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::model::{Oid, RepoPath};

use crate::document::{Classification, classify, excluded_by_built_in};
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
    root_tree: &Oid,
) -> Result<SnapshotDiscovery, Error> {
    let mut discovery = SnapshotDiscovery {
        documents: Vec::new(),
        outside_document_set: 0,
        tree_entries: 0,
        path_defects: Vec::new(),
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

        let Some(classification) = classify(&path) else {
            discovery.outside_document_set = discovery.outside_document_set.saturating_add(1);
            continue;
        };
        let status = side_status(repo, git, scan, classification, &path, &entry)?;
        discovery.documents.push(DocumentRecord {
            path,
            classification,
            status,
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
    classification: Classification,
    path: &str,
    entry: &TreeEntry,
) -> Result<DocumentStatus, Error> {
    if excluded_by_built_in(path) {
        return Ok(DocumentStatus::ExcludedBuiltIn);
    }
    match entry.mode {
        GitMode::Symlink => return Ok(DocumentStatus::Unsupported(UnsupportedKind::Symlink)),
        GitMode::Gitlink => return Ok(DocumentStatus::Unsupported(UnsupportedKind::Gitlink)),
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
                return Ok(DocumentStatus::Failed(defect));
            }
            return Err(defect);
        }
    };
    scan.charge_document_bytes(u64::try_from(object.body.len()).unwrap_or(u64::MAX))?;
    if lfs::is_pointer(&object.body) {
        return Ok(DocumentStatus::Unsupported(UnsupportedKind::LfsPointer));
    }
    match scan_bytes(scan, classification.adapter(), &object.body) {
        Ok(scanned) => Ok(DocumentStatus::Scanned(scanned)),
        Err(defect) if defect.is_document_scoped() => Ok(DocumentStatus::Failed(defect)),
        Err(defect) => Err(defect),
    }
}
