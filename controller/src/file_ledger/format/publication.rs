use sha2::Digest as _;
mod tests;

use amiss_wire::model::Digest;
use amiss_wire::model::Oid;
use amiss_wire::report::MACHINE_JSON_BYTES;
use serde::{Deserialize, Serialize};

use crate::{ArtifactReference, CheckBinding, ControllerEvaluationId, Publication};

use super::model::{StoredConclusion, StoredDelivery, StoredRun};
use crate::ProviderRunIdentity;
use crate::file_ledger::FileLedgerError;

const REPORT_DOMAIN: &str = "amiss/controller-report-blob-v1";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct StoredPublication {
    provider_run: ProviderRunIdentity,
    evaluation_id: ControllerEvaluationId,
    check: CheckBinding,
    run: StoredRun,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gate_commit: Option<Oid>,
    conclusion: StoredConclusion,
    report: StoredReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact: Option<ArtifactReference>,
}

impl StoredPublication {
    pub(in crate::file_ledger) fn new(publication: &Publication) -> Result<Self, FileLedgerError> {
        validate_artifact_report(publication.artifact.as_ref(), publication.report.as_deref())?;
        let report = StoredReport::new(publication.report.as_deref())?;
        Ok(Self {
            provider_run: publication.provider_run.clone(),
            evaluation_id: publication.evaluation_id.clone(),
            check: publication.check.clone(),
            run: StoredRun {
                change: super::model::StoredChange::new(&publication.run.change),
                refs: publication.run.refs.clone(),
                object_format: publication.run.object_format,
                commits: publication.run.commits.clone(),
                trees: publication.run.trees.clone(),
            },
            gate_commit: Some(publication.gate_commit.clone()),
            conclusion: StoredConclusion::new(publication.conclusion),
            report,
            artifact: publication.artifact.clone(),
        })
    }

    pub(in crate::file_ledger) fn report(&self) -> Option<&ReportRef> {
        match &self.report {
            StoredReport::Absent => None,
            StoredReport::Blob { reference } => Some(reference),
        }
    }

    pub(in crate::file_ledger) const fn has_gate_commit(&self) -> bool {
        self.gate_commit.is_some()
    }

    pub(in crate::file_ledger) fn materialize(
        self,
        report: Option<Vec<u8>>,
    ) -> Result<Publication, FileLedgerError> {
        self.validate_metadata()?;
        self.report.verify(report.as_deref())?;
        validate_artifact_report(self.artifact.as_ref(), report.as_deref())?;
        let Self {
            provider_run,
            evaluation_id,
            check,
            run,
            gate_commit: Some(gate_commit),
            conclusion,
            artifact,
            ..
        } = self
        else {
            return Err(FileLedgerError::Corrupt);
        };
        Ok(Publication {
            provider_run,
            evaluation_id,
            check,
            run: run.materialize()?,
            gate_commit,
            conclusion: conclusion.materialize(),
            report,
            artifact,
        })
    }

    fn validate_metadata(&self) -> Result<(), FileLedgerError> {
        if let Some(reference) = self.report() {
            reference.validate()?;
        }
        if self
            .artifact
            .as_ref()
            .is_some_and(|artifact| !crate::artifacts::valid_reference(artifact))
            || !self.provider_run.is_valid()
            || self
                .gate_commit
                .as_ref()
                .is_none_or(|commit| commit.object_format() != self.run.object_format)
        {
            return Err(FileLedgerError::Corrupt);
        }
        self.run.validate()
    }

    pub(super) fn validate_binding(
        &self,
        expected_evaluation_id: &ControllerEvaluationId,
        delivery: &StoredDelivery,
        expected_check: &CheckBinding,
    ) -> Result<(), FileLedgerError> {
        if self.artifact.is_some() && self.report().is_none() {
            return Err(FileLedgerError::Corrupt);
        }
        if self.has_gate_commit() {
            self.validate_metadata()?;
        } else {
            if self.artifact.is_some() {
                return Err(FileLedgerError::Corrupt);
            }
            if let Some(reference) = self.report() {
                reference.validate()?;
            }
            if !self.provider_run.is_valid() {
                return Err(FileLedgerError::Corrupt);
            }
            self.run.validate()?;
        }
        if self.evaluation_id != *expected_evaluation_id
            || self.provider_run != delivery.provider_run
            || self.run.change != delivery.change
            || self.run.object_format != delivery.provider_run.object_format
            || self.run.commits.candidate != delivery.provider_run.candidate_commit
            || self.check != *expected_check
        {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(())
    }
}

fn validate_artifact_report(
    artifact: Option<&ArtifactReference>,
    report: Option<&[u8]>,
) -> Result<(), FileLedgerError> {
    match artifact {
        Some(reference) if !crate::artifacts::reference_matches_report(reference, report) => {
            Err(FileLedgerError::Corrupt)
        }
        None | Some(_) => Ok(()),
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "report", rename_all = "kebab-case", deny_unknown_fields)]
enum StoredReport {
    Absent,
    Blob { reference: ReportRef },
}

impl StoredReport {
    fn new(report: Option<&[u8]>) -> Result<Self, FileLedgerError> {
        match report {
            Some(bytes) => ReportRef::new(bytes).map(|reference| Self::Blob { reference }),
            None => Ok(Self::Absent),
        }
    }

    fn verify(&self, report: Option<&[u8]>) -> Result<(), FileLedgerError> {
        match (self, report) {
            (Self::Absent, None) => Ok(()),
            (Self::Blob { reference }, Some(bytes)) if reference.matches(bytes) => Ok(()),
            (Self::Absent, Some(_)) | (Self::Blob { .. }, None | Some(_)) => {
                Err(FileLedgerError::Corrupt)
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct ReportRef {
    digest: Digest,
    length: u64,
}

impl ReportRef {
    pub(in crate::file_ledger) fn new(report: &[u8]) -> Result<Self, FileLedgerError> {
        report_length(report.len()).map(|length| Self {
            digest: sha2::Sha256::new_with_prefix(REPORT_DOMAIN)
                .chain_update([0_u8])
                .chain_update(report)
                .finalize()
                .0
                .into(),
            length,
        })
    }

    pub(in crate::file_ledger) fn matches(&self, report: &[u8]) -> bool {
        u64::try_from(report.len()).ok() == Some(self.length)
            && Self::new(report).is_ok_and(|actual| actual == *self)
    }

    fn validate(&self) -> Result<(), FileLedgerError> {
        if self.length > MACHINE_JSON_BYTES {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(())
    }
}

fn report_length(length: usize) -> Result<u64, FileLedgerError> {
    let length = u64::try_from(length).map_err(|_defect| FileLedgerError::ReportTooLarge)?;
    (length <= MACHINE_JSON_BYTES)
        .then_some(length)
        .ok_or(FileLedgerError::ReportTooLarge)
}
