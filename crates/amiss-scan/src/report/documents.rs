use amiss_wire::controls::{ContentAvailability, GitMode};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::model::{
    self, DocumentChange, DocumentEntryKind, DocumentResult, DocumentSide, UnsupportedReason,
};

use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};
use crate::document::Classification;

fn side_facets(
    record: &DocumentRecord,
) -> (
    model::DocumentStatus,
    Option<UnsupportedReason>,
    ContentAvailability,
    Option<Adapter>,
) {
    match &record.status {
        DocumentStatus::Scanned(_) => (
            model::DocumentStatus::Scanned,
            None,
            ContentAvailability::Available,
            record.adapter,
        ),
        DocumentStatus::ExcludedBuiltIn => (
            model::DocumentStatus::ExcludedBuiltIn,
            None,
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::LfsPointer) => (
            model::DocumentStatus::Unsupported,
            Some(UnsupportedReason::LfsPointer),
            ContentAvailability::LfsPointerOnly,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Symlink) => (
            model::DocumentStatus::Unsupported,
            Some(UnsupportedReason::SymlinkDocument),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Gitlink) => (
            model::DocumentStatus::Unsupported,
            Some(UnsupportedReason::GitlinkDocument),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Format) => (
            model::DocumentStatus::Unsupported,
            Some(UnsupportedReason::UnsupportedDocumentFormat),
            ContentAvailability::Available,
            None,
        ),
        DocumentStatus::Failed(_) => (
            model::DocumentStatus::Scanned,
            None,
            ContentAvailability::NotRead,
            None,
        ),
    }
}

fn document_side(record: &DocumentRecord) -> DocumentSide<GitMode> {
    let entry_kind = match record.mode {
        GitMode::Symlink => DocumentEntryKind::Symlink,
        GitMode::Gitlink => DocumentEntryKind::Gitlink,
        GitMode::RegularFile | GitMode::ExecutableFile | GitMode::Tree => DocumentEntryKind::Blob,
    };
    let (status, reason, availability, adapter) = side_facets(record);
    let scanned = match &record.status {
        DocumentStatus::Scanned(value) => Some(value),
        DocumentStatus::ExcludedBuiltIn
        | DocumentStatus::Unsupported(_)
        | DocumentStatus::Failed(_) => None,
    };
    let opaque = scanned.map(|value| &value.opaque);
    let count = |value: Option<usize>| u64::try_from(value.unwrap_or(0)).unwrap_or(u64::MAX);
    let byte_sum = |spans: Option<&Vec<(usize, usize)>>| {
        spans.map_or(0, |list| {
            list.iter()
                .map(|(start, end)| u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
                .sum::<u64>()
        })
    };
    DocumentSide {
        entry_kind,
        entry_oid: record.oid.clone(),
        git_mode: record.mode,
        raw_digest: record.raw_digest,
        status,
        unsupported_reason: reason,
        content_availability: availability,
        adapter_id: adapter,
        byte_count: record.byte_count,
        frontmatter_regions: u64::from(opaque.is_some_and(|value| value.frontmatter_bytes > 0)),
        frontmatter_bytes: count(opaque.map(|value| value.frontmatter_bytes)),
        opaque_mdx_regions: count(opaque.map(|value| value.mdx.len())),
        opaque_mdx_bytes: byte_sum(opaque.map(|value| &value.mdx)),
        opaque_html_regions: count(opaque.map(|value| value.html.len())),
        opaque_html_bytes: byte_sum(opaque.map(|value| &value.html)),
        extracted_references: scanned.map_or(0, |value| {
            u64::try_from(value.occurrences.len()).unwrap_or(u64::MAX)
        }),
    }
}

pub(super) struct PairedDocument<'a> {
    pub(super) path: RepoPath,
    classification: Classification,
    pub(super) base: Option<&'a DocumentRecord>,
    pub(super) candidate: Option<&'a DocumentRecord>,
}

pub(super) fn paired_documents<'a>(
    base: &'a SnapshotDiscovery,
    candidate: &'a SnapshotDiscovery,
) -> Vec<PairedDocument<'a>> {
    let mut paired = Vec::with_capacity(
        base.documents
            .len()
            .saturating_add(candidate.documents.len()),
    );
    let mut base_at = 0;
    let mut candidate_at = 0;
    while let (Some(base_record), Some(candidate_record)) = (
        base.documents.get(base_at),
        candidate.documents.get(candidate_at),
    ) {
        match base_record.path.cmp(&candidate_record.path) {
            std::cmp::Ordering::Less => {
                paired.push(paired_document(base_record, Some(base_record), None));
                base_at = base_at.saturating_add(1);
            }
            std::cmp::Ordering::Equal => {
                paired.push(paired_document(
                    candidate_record,
                    Some(base_record),
                    Some(candidate_record),
                ));
                base_at = base_at.saturating_add(1);
                candidate_at = candidate_at.saturating_add(1);
            }
            std::cmp::Ordering::Greater => {
                paired.push(paired_document(
                    candidate_record,
                    None,
                    Some(candidate_record),
                ));
                candidate_at = candidate_at.saturating_add(1);
            }
        }
    }
    if let Some(remaining) = base.documents.get(base_at..) {
        paired.extend(
            remaining
                .iter()
                .map(|record| paired_document(record, Some(record), None)),
        );
    }
    if let Some(remaining) = candidate.documents.get(candidate_at..) {
        paired.extend(
            remaining
                .iter()
                .map(|record| paired_document(record, None, Some(record))),
        );
    }
    paired
}

fn paired_document<'a>(
    record: &DocumentRecord,
    base: Option<&'a DocumentRecord>,
    candidate: Option<&'a DocumentRecord>,
) -> PairedDocument<'a> {
    PairedDocument {
        path: record.path.clone(),
        classification: record.classification,
        base,
        candidate,
    }
}

pub(super) fn document_result(
    paired: &PairedDocument<'_>,
) -> DocumentResult<RepoPath, DocumentSide<GitMode>> {
    let base = paired.base.map(document_side);
    let candidate = paired.candidate.map(document_side);
    let change = match (&base, &candidate) {
        (None, None) => DocumentChange::Unchanged,
        (None, Some(_)) => DocumentChange::Added,
        (Some(_), None) => DocumentChange::Removed,
        (Some(left), Some(right)) if left == right => DocumentChange::Unchanged,
        (Some(_), Some(_)) => DocumentChange::Changed,
    };
    DocumentResult {
        path: paired.path.clone(),
        classification: paired.classification,
        base,
        candidate,
        change,
    }
}
