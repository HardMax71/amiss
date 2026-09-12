#![cfg(test)]

use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::file_ledger::FileLedgerError;

use super::{ReportRef, report_length};

#[test]
fn report_lengths_include_the_ceiling_and_exclude_the_next_byte() {
    let ceiling = usize::try_from(MACHINE_JSON_BYTES).expect("the ceiling fits this host");
    let oversized = usize::try_from(MACHINE_JSON_BYTES.saturating_add(1))
        .expect("the first oversized length fits this host");

    assert!(matches!(
        report_length(ceiling),
        Ok(length) if length == MACHINE_JSON_BYTES
    ));
    assert!(matches!(
        report_length(oversized),
        Err(FileLedgerError::ReportTooLarge)
    ));
}

#[test]
fn persisted_report_identity_keeps_its_domain_and_serialized_shape() {
    let reference = ReportRef::new(b"the very report").expect("a report reference");
    let golden = r#"{"digest":"sha256:b7f842692bad2ed5bc52e869ba4ea9c2bc5871a7f4ef0ebe243e768e13a7b1df","length":15}"#;
    assert_eq!(
        serde_json::to_string(&reference).expect("serialize reference"),
        golden
    );
    let loaded: ReportRef = serde_json::from_str(golden).expect("read persisted reference");
    assert!(loaded.matches(b"the very report"));
    assert!(!loaded.matches(b"another report!"));
}
