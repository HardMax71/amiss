use amiss_scan::{ScanLimits, ScanResources};
use amiss_wire::model::{Adapter, ObjectFormat};

/// Strict JSON: parsing either rejects or yields a value whose canonical
/// form reparses to the same value, canonicalization is idempotent, and the
/// streaming serializer with its counting pass agrees byte for byte.
///
/// # Panics
///
/// Panics when an accepted input violates a JSON invariant.
#[expect(
    clippy::expect_used,
    reason = "a canonical-output parse failure is a fuzz finding"
)]
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
    let mut streamed = String::new();
    amiss_wire::json::stream(&value, &mut streamed);
    assert_eq!(
        streamed.as_bytes(),
        canonical.as_slice(),
        "streaming equals materialization"
    );
    assert_eq!(
        amiss_wire::json::canonical_length(&value),
        u64::try_from(canonical.len()).unwrap_or(u64::MAX),
        "the counting pass reports the exact length"
    );
}

/// Every control parser over the same bytes: no panic escapes, and parsing
/// twice yields identical results.
///
/// # Panics
///
/// Panics when a control parser is nondeterministic.
pub fn controls(bytes: &[u8]) {
    assert_eq!(
        amiss_wire::controls::parse_scanner_policy(bytes),
        amiss_wire::controls::parse_scanner_policy(bytes),
    );
    assert_eq!(
        amiss_wire::controls::parse_organization_floor(bytes),
        amiss_wire::controls::parse_organization_floor(bytes),
    );
    assert_eq!(
        amiss_wire::controls::parse_debt_snapshot(bytes),
        amiss_wire::controls::parse_debt_snapshot(bytes),
    );
    assert_eq!(
        amiss_wire::controls::parse_waiver_bundle(bytes),
        amiss_wire::controls::parse_waiver_bundle(bytes),
    );
    assert_eq!(
        amiss_wire::controls::parse_trusted_time(bytes),
        amiss_wire::controls::parse_trusted_time(bytes),
    );
    assert_eq!(
        amiss_wire::controls::parse_execution_constraint(bytes),
        amiss_wire::controls::parse_execution_constraint(bytes),
    );
}

/// The three request parsers: no panic escapes, and parsing is
/// deterministic.
///
/// # Panics
///
/// Panics when a request parser is nondeterministic.
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
///
/// # Panics
///
/// Panics when an accepted document violates a parser invariant.
pub fn markdown(bytes: &[u8]) {
    for adapter in [
        Adapter::Markdown,
        Adapter::Mdx,
        Adapter::AsciiDoc,
        Adapter::Rst,
    ] {
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
///
/// # Panics
///
/// Panics when the index parser is nondeterministic.
pub fn git_index(bytes: &[u8]) {
    for format in [ObjectFormat::Sha1, ObjectFormat::Sha256] {
        let first = amiss_git::parse_index_file(format, bytes).is_ok();
        let second = amiss_git::parse_index_file(format, bytes).is_ok();
        assert_eq!(first, second, "parsing is deterministic");
    }
}

/// The commit and tree body grammars in both object formats: no panic
/// escapes, and accepted trees obey the entry laws.
///
/// # Panics
///
/// Panics when an accepted tree violates an entry invariant.
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
///
/// # Panics
///
/// Panics when rendered output violates a human-atom invariant.
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

/// The reserved claim grammar over scanned markdown: an accepted value claim
/// keeps the closed grammar's own promises, a name inside its charset and
/// length, a path `RepoPath` accepts again, and a line inside the safe
/// integer window, while governed spans stay inside the source.
///
/// # Panics
///
/// Panics when an accepted claim escapes the closed grammar.
pub fn claim(bytes: &[u8]) {
    for adapter in [Adapter::Markdown, Adapter::Rst, Adapter::AsciiDoc] {
        claim_under(adapter, bytes);
    }
}

#[expect(
    clippy::expect_used,
    reason = "a claim outside its own grammar is a fuzz finding"
)]
fn claim_under(adapter: Adapter, bytes: &[u8]) {
    let mut resources = ScanResources::new(ScanLimits::CONTRACT);
    let Ok(scanned) = amiss_scan::scan_document(&mut resources, adapter, bytes) else {
        return;
    };
    for source in &scanned.governed {
        let (start, end) = source.span;
        assert!(
            start <= end && end <= bytes.len(),
            "governed spans stay inside the source"
        );
        let amiss_scan::claim::GovernedForm::Value(claim) = &source.form else {
            continue;
        };
        assert!(
            !claim.name.is_empty() && claim.name.len() <= 120,
            "a claim name keeps the closed length"
        );
        assert!(
            claim
                .name
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric()),
            "a claim name opens alphanumeric"
        );
        assert!(
            claim
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
            "a claim name stays inside its charset"
        );
        let text = String::from_utf8(claim.path.as_bytes().to_vec()).expect("a claim path is text");
        assert!(
            amiss_wire::model::RepoPath::new(text).is_some(),
            "a claim path revalidates"
        );
        let ceiling = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER).unwrap_or(u64::MAX);
        assert!(
            (1..=ceiling).contains(&claim.line),
            "a claim line stays inside the safe window"
        );
    }
}
