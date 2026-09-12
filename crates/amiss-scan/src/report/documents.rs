use amiss_wire::controls::ContentAvailability;
use amiss_wire::digest::Digest;
use amiss_wire::model::{Adapter, RepoPath};
use serde::Serialize;

use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};

fn side_facets(
    record: &DocumentRecord,
) -> (
    &'static str,
    Option<&'static str>,
    ContentAvailability,
    Option<Adapter>,
) {
    match &record.status {
        DocumentStatus::Scanned(_) => (
            "scanned",
            None,
            ContentAvailability::Available,
            record.adapter,
        ),
        DocumentStatus::ExcludedBuiltIn => (
            "excluded-built-in",
            None,
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::LfsPointer) => (
            "unsupported",
            Some("lfs-pointer"),
            ContentAvailability::LfsPointerOnly,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Symlink) => (
            "unsupported",
            Some("symlink-document"),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Gitlink) => (
            "unsupported",
            Some("gitlink-document"),
            ContentAvailability::NotRead,
            None,
        ),
        DocumentStatus::Unsupported(UnsupportedKind::Format) => (
            "unsupported",
            Some("unsupported-document-format"),
            ContentAvailability::Available,
            None,
        ),
        DocumentStatus::Failed(_) => ("scanned", None, ContentAvailability::NotRead, None),
    }
}

#[derive(PartialEq, Serialize)]
struct DocumentSide<'a> {
    adapter_id: Option<Adapter>,
    byte_count: u64,
    content_availability: ContentAvailability,
    entry_kind: &'static str,
    entry_oid: &'a str,
    extracted_references: u64,
    frontmatter_bytes: u64,
    frontmatter_regions: u64,
    git_mode: amiss_wire::controls::GitMode,
    opaque_html_bytes: u64,
    opaque_html_regions: u64,
    opaque_mdx_bytes: u64,
    opaque_mdx_regions: u64,
    raw_digest: Option<Digest>,
    status: &'static str,
    unsupported_reason: Option<&'static str>,
}

fn document_side(record: &DocumentRecord) -> DocumentSide<'_> {
    let entry_kind = match record.mode {
        amiss_wire::controls::GitMode::Symlink => "symlink",
        amiss_wire::controls::GitMode::Gitlink => "gitlink",
        amiss_wire::controls::GitMode::RegularFile
        | amiss_wire::controls::GitMode::ExecutableFile
        | amiss_wire::controls::GitMode::Tree => "blob",
    };
    let (status, unsupported_reason, content_availability, adapter_id) = side_facets(record);
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
            list.iter().fold(0_u64, |total, (start, end)| {
                total.saturating_add(u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
            })
        })
    };
    DocumentSide {
        adapter_id,
        byte_count: record.byte_count,
        content_availability,
        entry_kind,
        entry_oid: record.oid.as_str(),
        extracted_references: count(scanned.map(|value| value.occurrences.len())),
        frontmatter_bytes: count(opaque.map(|value| value.frontmatter_bytes)),
        frontmatter_regions: u64::from(opaque.is_some_and(|value| value.frontmatter_bytes > 0)),
        git_mode: record.mode,
        opaque_html_bytes: byte_sum(opaque.map(|value| &value.html)),
        opaque_html_regions: count(opaque.map(|value| value.html.len())),
        opaque_mdx_bytes: byte_sum(opaque.map(|value| &value.mdx)),
        opaque_mdx_regions: count(opaque.map(|value| value.mdx.len())),
        raw_digest: record.raw_digest,
        status,
        unsupported_reason,
    }
}

pub(super) struct PairedDocument<'a> {
    pub(super) path: RepoPath,
    classification: &'static str,
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
        classification: record.classification.into(),
        base,
        candidate,
    }
}

#[derive(Serialize)]
pub(super) struct DocumentResult<'a> {
    base: Option<DocumentSide<'a>>,
    candidate: Option<DocumentSide<'a>>,
    change: &'static str,
    classification: &'static str,
    path: &'a RepoPath,
}

pub(super) fn document_result_value<'a>(paired: &'a PairedDocument<'_>) -> DocumentResult<'a> {
    let base = paired.base.map(document_side);
    let candidate = paired.candidate.map(document_side);
    let change = match (&base, &candidate) {
        (None, None) => "unchanged",
        (None, Some(_)) => "added",
        (Some(_), None) => "removed",
        (Some(left), Some(right)) if left == right => "unchanged",
        (Some(_), Some(_)) => "changed",
    };
    DocumentResult {
        base,
        candidate,
        change,
        classification: paired.classification,
        path: &paired.path,
    }
}
