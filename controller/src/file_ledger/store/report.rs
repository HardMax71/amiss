mod tests;

use std::io;

use amiss_wire::report::MACHINE_JSON_BYTES;

use super::{Row, atomic_write, read_bounded, reject_non_file};
use crate::file_ledger::FileLedgerError;
use crate::file_ledger::format::ReportRef;

impl Row {
    pub(in crate::file_ledger) fn save_report(
        &self,
        report: Option<&[u8]>,
        reference: Option<&ReportRef>,
    ) -> Result<(), FileLedgerError> {
        let report = match (report, reference) {
            (None, None) => return Ok(()),
            (Some(report), Some(reference)) if reference.matches(report) => report,
            (None, Some(_)) | (Some(_), None | Some(_)) => {
                return Err(FileLedgerError::Corrupt);
            }
        };
        let path = self.root.join(format!("{}.report", self.key));
        match read_bounded(&path, MACHINE_JSON_BYTES) {
            Ok(existing) if existing == report => Ok(()),
            Ok(_) => Err(FileLedgerError::Corrupt),
            Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                atomic_write(&path, report)
            }
            Err(error) => Err(error),
        }
    }

    pub(in crate::file_ledger) fn remove_report(&self) -> Result<(), FileLedgerError> {
        let path = self.root.join(format!("{}.report", self.key));
        reject_non_file(&path)?;
        std::fs::remove_file(path)
            .or_else(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(Into::into)
    }
}
