use std::collections::BTreeSet;
use std::sync::Arc;

use sha2::Digest as _;

use amiss_wire::controls::GitMode;
use amiss_wire::model::{Adapter, Oid, RepoPath};

use super::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind, collect_labels};
use crate::document::Classification;
use crate::resources::{ScanIdentity, ScanResources};
use crate::scan::{PureFailure, Scanned, replay_scan_charges, scan_pure};
use crate::workers::{Reply, Task};
use crate::{Error, lfs};

/// How many documents a walk holds back before settling them, and how many
/// blob bytes those documents may hold, so a batch keeps every worker busy
/// while the bodies in memory stay bounded.
const BATCH_DOCUMENTS: usize = 256;
const BATCH_BYTES: usize = 32 * 1024 * 1024;

/// The documents a walk has staged and not yet settled.
#[derive(Default)]
pub(super) struct Batch {
    staged: Vec<Staged>,
    held: usize,
}

impl Batch {
    /// Takes one staged document and settles the batch once it is full.
    pub(super) fn stage(
        &mut self,
        staged: Staged,
        discovery: &mut SnapshotDiscovery,
        scan: &mut ScanResources,
    ) -> Result<(), Error> {
        self.held = self.held.saturating_add(staged.held_bytes());
        self.staged.push(staged);
        if self.staged.len() < BATCH_DOCUMENTS && self.held < BATCH_BYTES {
            return Ok(());
        }
        self.held = 0;
        settle(discovery, scan, std::mem::take(&mut self.staged))
    }

    pub(super) fn finish(
        self,
        discovery: &mut SnapshotDiscovery,
        scan: &mut ScanResources,
    ) -> Result<(), Error> {
        settle(discovery, scan, self.staged)
    }
}

/// A document the walk classified: settled on the spot, or read and waiting
/// for its parse and its charges.
pub(super) enum Staged {
    Settled(DocumentRecord),
    Pending(Pending),
}

impl Staged {
    pub(super) fn held_bytes(&self) -> usize {
        match self {
            Self::Settled(_) => 0,
            Self::Pending(pending) => pending.read.as_ref().map_or(0, |body| body.len()),
        }
    }
}

/// One admitted regular blob between its read and its settlement. The read
/// keeps its defect, which settlement raises in tree order as the walk did.
pub(super) struct Pending {
    pub(super) path: RepoPath,
    pub(super) classification: Classification,
    pub(super) adapter: Option<Adapter>,
    pub(super) oid: Oid,
    pub(super) mode: GitMode,
    pub(super) read: Result<Arc<[u8]>, Error>,
}

type Outcome = Option<Result<Scanned, PureFailure>>;

/// Parses the batch on every worker, then settles each document in tree
/// order: admission, byte charges, pointer and format checks, the parse's
/// charges, labels and the record, exactly the sequence one document went
/// through when the walk scanned it in place.
fn settle(
    discovery: &mut SnapshotDiscovery,
    scan: &mut ScanResources,
    batch: Vec<Staged>,
) -> Result<(), Error> {
    let mut job_of = Vec::with_capacity(batch.len());
    let mut outcomes: Vec<Outcome> = Vec::new();
    {
        let mut tasks = Vec::new();
        let mut seen = BTreeSet::new();
        for staged in &batch {
            let job = match staged {
                Staged::Pending(Pending {
                    read: Ok(body),
                    adapter: Some(adapter),
                    oid,
                    ..
                }) if *adapter != Adapter::Mdx
                    && !lfs::is_pointer(body)
                    && seen.insert((oid.clone(), *adapter)) =>
                {
                    let position = tasks.len();
                    tasks.push(Task {
                        position,
                        adapter: *adapter,
                        body: Arc::clone(body),
                        allowance: scan.embedded_code_allowance(),
                    });
                    Some(position)
                }
                Staged::Settled(_) | Staged::Pending(_) => None,
            };
            job_of.push(job);
        }
        outcomes.resize_with(tasks.len(), || None);
        for Reply { position, outcome } in crate::workers::run(tasks, scan.workers()) {
            if let Some(slot) = outcomes.get_mut(position) {
                *slot = Some(outcome);
            }
        }
    }
    for (staged, job) in batch.into_iter().zip(job_of) {
        match staged {
            Staged::Settled(record) => push(discovery, scan, record)?,
            Staged::Pending(pending) => {
                let prepared = job
                    .and_then(|index| outcomes.get_mut(index))
                    .and_then(Option::take);
                settle_pending(discovery, scan, pending, prepared)?;
            }
        }
    }
    Ok(())
}

fn settle_pending(
    discovery: &mut SnapshotDiscovery,
    scan: &mut ScanResources,
    pending: Pending,
    prepared: Outcome,
) -> Result<(), Error> {
    let Pending {
        path,
        classification,
        adapter,
        oid,
        mode,
        read,
    } = pending;
    let record = |status, byte_count, raw_digest| DocumentRecord {
        path,
        classification,
        adapter,
        status,
        oid: oid.clone(),
        mode,
        byte_count,
        raw_digest,
    };
    scan.admit_document()?;
    let body = match read {
        Ok(body) => body,
        Err(defect) if defect.is_document_scoped() => {
            return push(
                discovery,
                scan,
                record(DocumentStatus::Failed(defect), 0, None),
            );
        }
        Err(defect) => return Err(defect),
    };
    let byte_count = u64::try_from(body.len()).unwrap_or(u64::MAX);
    scan.charge_document_bytes(byte_count)?;
    let raw = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(crate::resolve::RAW_EVIDENCE_DOMAIN)
            .chain_update([0_u8])
            .chain_update(&body)
            .finalize()
            .0,
    );
    if lfs::is_pointer(&body) {
        let status = DocumentStatus::Unsupported(UnsupportedKind::LfsPointer);
        return push(discovery, scan, record(status, byte_count, Some(raw)));
    }
    let Some(adapter) = adapter else {
        let status = DocumentStatus::Unsupported(UnsupportedKind::Format);
        return push(discovery, scan, record(status, byte_count, Some(raw)));
    };
    let identity = ScanIdentity {
        oid: oid.clone(),
        adapter,
        embedded_code_allowance: (adapter == Adapter::Mdx).then(|| scan.embedded_code_allowance()),
    };
    let scanned = if let Some(scanned) = scan.scans.get(&identity).cloned() {
        replay_scan_charges(scan, &scanned).map(|()| scanned)
    } else {
        let outcome =
            prepared.unwrap_or_else(|| scan_pure(adapter, &body, scan.embedded_code_allowance()));
        let fresh = match outcome {
            Ok(scanned) => replay_scan_charges(scan, &scanned).map(|()| Arc::new(scanned)),
            Err(failure) => Err(failure.into_error(scan)),
        };
        fresh.inspect(|scanned| {
            scan.scans.insert(identity, Arc::clone(scanned));
        })
    };
    let status = match scanned {
        Ok(scanned) => DocumentStatus::Scanned(scanned),
        Err(defect) if defect.is_document_scoped() => DocumentStatus::Failed(defect),
        Err(defect) => return Err(defect),
    };
    push(discovery, scan, record(status, byte_count, Some(raw)))
}

fn push(
    discovery: &mut SnapshotDiscovery,
    scan: &mut ScanResources,
    record: DocumentRecord,
) -> Result<(), Error> {
    collect_labels(scan, &mut discovery.labels, &record.path, &record.status)?;
    discovery.documents.push(record);
    Ok(())
}
