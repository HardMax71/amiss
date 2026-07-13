use amiss_scan::{ScanLimits, ScanResources};
use amiss_wire::model::{Adapter, ObjectFormat};

/// Strict JSON: parsing either rejects or yields a value whose canonical
/// form reparses to the same value, and canonicalization is idempotent.
pub fn json(bytes: &[u8]) {
    let Ok(value) = amiss_wire::json::parse(bytes) else {
        return;
    };
    let canonical = amiss_wire::json::canonical(&value);
    let reparsed = amiss_wire::json::parse(&canonical).expect("canonical bytes reparse");
    assert_eq!(reparsed, value, "canonicalization preserves the value");
    assert_eq!(
        amiss_wire::json::canonical(&reparsed),
        canonical,
        "canonicalization is idempotent"
    );
}

/// Every control parser over the same bytes: no panic escapes, and parsing
/// twice yields identical results.
pub fn controls(bytes: &[u8]) {
    assert_eq!(
        amiss_wire::controls::ScannerPolicy::parse(bytes),
        amiss_wire::controls::ScannerPolicy::parse(bytes),
    );
    assert_eq!(
        amiss_wire::controls::OrganizationFloor::parse(bytes),
        amiss_wire::controls::OrganizationFloor::parse(bytes),
    );
    assert_eq!(
        amiss_wire::controls::DebtSnapshot::parse(bytes),
        amiss_wire::controls::DebtSnapshot::parse(bytes),
    );
    assert_eq!(
        amiss_wire::controls::WaiverBundle::parse(bytes),
        amiss_wire::controls::WaiverBundle::parse(bytes),
    );
    assert_eq!(
        amiss_wire::controls::TrustedTimeStatement::parse(bytes),
        amiss_wire::controls::TrustedTimeStatement::parse(bytes),
    );
    assert_eq!(
        amiss_wire::controls::ExecutionConstraintDescriptor::parse(bytes),
        amiss_wire::controls::ExecutionConstraintDescriptor::parse(bytes),
    );
}

/// The three request parsers: no panic escapes, and parsing is
/// deterministic.
pub fn requests(bytes: &[u8]) {
    assert_eq!(
        amiss_wire::requests::EvaluationRequest::parse(bytes),
        amiss_wire::requests::EvaluationRequest::parse(bytes),
    );
    assert_eq!(
        amiss_wire::requests::SnapshotRequest::parse(bytes),
        amiss_wire::requests::SnapshotRequest::parse(bytes),
    );
    assert_eq!(
        amiss_wire::requests::ControlsRequest::parse(bytes),
        amiss_wire::requests::ControlsRequest::parse(bytes),
    );
}

/// Both document adapters under the contract ceilings: a parser panic is
/// classified, never escaping; every span stays inside the source; the
/// reference budget holds.
pub fn markdown(bytes: &[u8]) {
    for adapter in [Adapter::Markdown, Adapter::Mdx] {
        let mut resources = ScanResources::new(ScanLimits::CONTRACT);
        let first = amiss_scan::scan_document(&mut resources, adapter, bytes);
        let mut again = ScanResources::new(ScanLimits::CONTRACT);
        let second = amiss_scan::scan_document(&mut again, adapter, bytes);
        assert_eq!(first.is_ok(), second.is_ok(), "parsing is deterministic");
        let Ok(scanned) = first else {
            continue;
        };
        assert!(
            u64::try_from(scanned.occurrences.len()).unwrap_or(u64::MAX)
                <= ScanLimits::CONTRACT.references_per_document,
            "the per-document reference budget holds"
        );
        for occurrence in &scanned.occurrences {
            let (start, end) = occurrence.occurrence.span;
            assert!(start <= end, "spans are ordered");
            assert!(end <= bytes.len(), "spans stay inside the source");
        }
    }
}

/// The index-file grammar in both object formats: no panic escapes, and
/// parsing is deterministic.
pub fn git_index(bytes: &[u8]) {
    for format in [ObjectFormat::Sha1, ObjectFormat::Sha256] {
        let first = amiss_git::parse_index_file(format, bytes).is_ok();
        let second = amiss_git::parse_index_file(format, bytes).is_ok();
        assert_eq!(first, second, "parsing is deterministic");
    }
}

/// The commit and tree body grammars in both object formats: no panic
/// escapes, and accepted trees obey the entry laws.
pub fn git_objects(bytes: &[u8]) {
    for format in [ObjectFormat::Sha1, ObjectFormat::Sha256] {
        let _commit = amiss_git::parse_commit(format, bytes);
        if let Ok(entries) = amiss_git::parse_tree(format, bytes) {
            for entry in &entries {
                assert!(!entry.name.is_empty(), "tree names are nonempty");
                assert!(
                    !entry.name.contains(&0) && !entry.name.contains(&b'/'),
                    "tree names exclude NUL and slash"
                );
            }
        }
    }
}

/// The human atom renderer: bounded output for any input, quoted, with the
/// 200-scalar law. A retained non-BMP scalar escapes to a surrogate pair of
/// twelve output characters, the widest single-scalar expansion.
pub fn human(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    let atom = amiss_wire::human::atom(&text);
    assert!(
        atom.starts_with('"') && atom.ends_with('"'),
        "atoms are quoted"
    );
    assert!(
        atom.chars().count() <= 2 + 200 * 12 + 3,
        "the atom bound holds: {} scalars",
        atom.chars().count()
    );
    if text.chars().count() > 200 {
        assert!(
            atom.ends_with("...\""),
            "omission is disclosed inside the quotes"
        );
    }
}
