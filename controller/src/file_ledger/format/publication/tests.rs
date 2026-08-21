#![cfg(test)]

use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::file_ledger::FileLedgerError;

use super::report_length;

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
