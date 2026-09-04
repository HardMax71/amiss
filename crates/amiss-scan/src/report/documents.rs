use amiss_wire::controls::ContentAvailability;
use amiss_wire::json::Value;
use amiss_wire::model::{Adapter, RepoPath};

use super::{digest_value, integer, nullable, object, string};
use crate::discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind};
use crate::document::Classification;

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

fn document_side_value(record: Option<&DocumentRecord>) -> Value {
    let Some(record) = record else {
        return Value::Null;
    };
    let entry_kind = match record.mode {
        amiss_wire::controls::GitMode::Symlink => "symlink",
        amiss_wire::controls::GitMode::Gitlink => "gitlink",
        amiss_wire::controls::GitMode::RegularFile
        | amiss_wire::controls::GitMode::ExecutableFile
        | amiss_wire::controls::GitMode::Tree => "blob",
    };
    let (status, reason, availability, adapter) = side_facets(record);
    let scanned = match &record.status {
        DocumentStatus::Scanned(value) => Some(value),
        DocumentStatus::ExcludedBuiltIn
        | DocumentStatus::Unsupported(_)
        | DocumentStatus::Failed(_) => None,
    };
    let opaque = scanned.map(|value| &value.opaque);
    let count =
        |value: Option<usize>| integer(u64::try_from(value.unwrap_or(0)).unwrap_or(u64::MAX));
    let byte_sum = |spans: Option<&Vec<(usize, usize)>>| {
        integer(spans.map_or(0, |list| {
            list.iter()
                .map(|(start, end)| u64::try_from(end.saturating_sub(*start)).unwrap_or(u64::MAX))
                .sum::<u64>()
        }))
    };
    object(vec![
        ("entry_kind", string(entry_kind)),
        ("entry_oid", string(record.oid.as_str())),
        ("git_mode", string(record.mode.as_ref())),
        (
            "raw_digest",
            record.raw_digest.map_or(Value::Null, digest_value),
        ),
        ("status", string(status)),
        ("unsupported_reason", nullable(reason)),
        ("content_availability", string(availability.as_ref())),
        (
            "adapter_id",
            adapter.map_or(Value::Null, |value: Adapter| string(value.as_ref())),
        ),
        ("byte_count", integer(record.byte_count)),
        (
            "frontmatter_regions",
            integer(
                opaque
                    .is_some_and(|value| value.frontmatter_bytes > 0)
                    .into(),
            ),
        ),
        (
            "frontmatter_bytes",
            count(opaque.map(|value| value.frontmatter_bytes)),
        ),
        (
            "opaque_mdx_regions",
            count(opaque.map(|value| value.mdx.len())),
        ),
        ("opaque_mdx_bytes", byte_sum(opaque.map(|value| &value.mdx))),
        (
            "opaque_html_regions",
            count(opaque.map(|value| value.html.len())),
        ),
        (
            "opaque_html_bytes",
            byte_sum(opaque.map(|value| &value.html)),
        ),
        (
            "extracted_references",
            integer(scanned.map_or(0, |value| {
                u64::try_from(value.occurrences.len()).unwrap_or(u64::MAX)
            })),
        ),
    ])
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

pub(super) fn document_result_value(paired: &PairedDocument<'_>) -> Value {
    let base = document_side_value(paired.base);
    let candidate = document_side_value(paired.candidate);
    let change = match (&base, &candidate) {
        (Value::Null, Value::Null) => "unchanged",
        (Value::Null, _present) => "added",
        (_present, Value::Null) => "removed",
        (left, right) if left == right => "unchanged",
        _ => "changed",
    };
    object(vec![
        ("path", paired.path.to_value()),
        ("classification", string(paired.classification.as_ref())),
        ("base", base),
        ("candidate", candidate),
        ("change", string(change)),
    ])
}
