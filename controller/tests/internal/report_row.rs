#![cfg(test)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::file_ledger::FileLedgerError;
use crate::file_ledger::format::ReportRef;
use crate::{FileLedgerConfig, ReplayWindow};

use super::super::Row;

fn row(root: &Path) -> Row {
    let window =
        ReplayWindow::new(Duration::from_mins(1), Duration::from_secs(10)).expect("a window");
    Row {
        root: root.to_path_buf(),
        key: "row-key".to_owned(),
        config: FileLedgerConfig::new(Duration::from_secs(1), 8, window).expect("a config"),
        _row_lock: tempfile::tempfile().expect("a lock handle"),
        _maintenance: tempfile::tempfile().expect("a lock handle"),
    }
}

/// A saved report answers to the reference that names it: the same bytes may
/// land twice, and anything else at the path is corruption, never a slot.
#[test]
fn a_report_answers_only_to_its_own_reference() {
    let dir = tempfile::tempdir().expect("a scratch directory");
    let row = row(dir.path());
    let report = b"the very report";
    let reference = ReportRef::new(report).expect("a reference");

    row.save_report(None, None).expect("nothing to persist");
    assert!(
        matches!(
            row.save_report(
                Some(report),
                Some(&ReportRef::new(b"another report").expect("a reference")),
            ),
            Err(FileLedgerError::Corrupt)
        ),
        "a report its reference does not name"
    );

    row.save_report(Some(report), Some(&reference))
        .expect("the first save writes");
    row.save_report(Some(report), Some(&reference))
        .expect("the same bytes again are idempotent");

    fs::write(dir.path().join("row-key.report"), b"someone else's bytes").expect("write");
    assert!(
        matches!(
            row.save_report(Some(report), Some(&reference)),
            Err(FileLedgerError::Corrupt)
        ),
        "other bytes at the path are corruption, not a slot to overwrite"
    );
}
