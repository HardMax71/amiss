use std::io;
use std::path::PathBuf;

use amiss_wire::report::MACHINE_JSON_BYTES;

use super::{Row, atomic_write, read_bounded};
use crate::file_ledger::FileLedgerError;
use crate::file_ledger::format::{self, ReportRef};

impl Row {
    pub(in crate::file_ledger) fn save_report(
        &self,
        report: Option<&[u8]>,
        reference: Option<&ReportRef>,
    ) -> Result<(), FileLedgerError> {
        match (report, reference) {
            (None, None) => Ok(()),
            (Some(report), Some(reference)) if reference.matches(report) => {
                let path = self.report_path(reference)?;
                match read_bounded(&path, MACHINE_JSON_BYTES) {
                    Ok(existing) if existing == report => Ok(()),
                    Ok(_) => Err(FileLedgerError::Corrupt),
                    Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                        atomic_write(&path, report)
                    }
                    Err(error) => Err(error),
                }
            }
            (None, Some(_)) | (Some(_), None | Some(_)) => Err(FileLedgerError::Corrupt),
        }
    }

    pub(in crate::file_ledger) fn load_report(
        &self,
        reference: Option<&ReportRef>,
    ) -> Result<Option<Vec<u8>>, FileLedgerError> {
        let Some(reference) = reference else {
            return Ok(None);
        };
        let bytes = match read_bounded(&self.report_path(reference)?, MACHINE_JSON_BYTES) {
            Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Err(FileLedgerError::Corrupt);
            }
            result => result?,
        };
        if !reference.matches(&bytes) {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(Some(bytes))
    }

    fn report_path(&self, reference: &ReportRef) -> Result<PathBuf, FileLedgerError> {
        let digest = format::digest_hex(reference.digest())?;
        Ok(self.root.join(format!("{}.report-{digest}", self.key)))
    }
}
