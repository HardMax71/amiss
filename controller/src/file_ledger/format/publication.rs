mod tests;

use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::Oid;
use amiss_wire::report::MACHINE_JSON_BYTES;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactReference, AuthenticatedDelivery, CheckBinding, ControllerEvaluationId, ExternalTally,
    Publication,
};

use super::model::{
    StoredCheck, StoredConclusion, StoredProviderRun, StoredRun, materialize_check, store_check,
};
use crate::file_ledger::FileLedgerError;

const REPORT_DOMAIN: &str = "amiss/controller-report-blob-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct StoredPublication {
    provider_run: StoredProviderRun,
    evaluation_id: String,
    check: StoredCheck,
    run: StoredRun,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gate_commit: Option<String>,
    conclusion: StoredConclusion,
    report: StoredReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact: Option<StoredArtifact>,
}

impl StoredPublication {
    pub(in crate::file_ledger) fn new(publication: &Publication) -> Result<Self, FileLedgerError> {
        validate_artifact_report(publication.artifact.as_ref(), publication.report.as_deref())?;
        let report = StoredReport::new(publication.report.as_deref())?;
        Ok(Self {
            provider_run: StoredProviderRun::new(&publication.provider_run),
            evaluation_id: publication.evaluation_id.as_str().to_owned(),
            check: store_check(&publication.check),
            run: StoredRun::new(&publication.run),
            gate_commit: Some(publication.gate_commit.as_str().to_owned()),
            conclusion: StoredConclusion::new(publication.conclusion),
            report,
            artifact: publication.artifact.as_ref().map(StoredArtifact::new),
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
        &self,
        report: Option<Vec<u8>>,
    ) -> Result<Publication, FileLedgerError> {
        let publication = self.report.attach(self.materialize_metadata()?, report)?;
        validate_artifact_report(publication.artifact.as_ref(), publication.report.as_deref())?;
        Ok(publication)
    }

    pub(super) fn materialize_metadata(&self) -> Result<Publication, FileLedgerError> {
        if let Some(reference) = self.report() {
            reference.validate()?;
        }
        let artifact = match &self.artifact {
            Some(stored) => {
                let assessment_digest = match stored.assessment_digest.as_deref() {
                    Some(raw) => Some(Digest::from_wire(raw).ok_or(FileLedgerError::Corrupt)?),
                    None => None,
                };
                Some(
                    crate::artifacts::checked_reference(
                        stored.id.clone(),
                        stored.locator.clone(),
                        stored.expires_at_unix_millis,
                        Digest::from_wire(&stored.report_digest).ok_or(FileLedgerError::Corrupt)?,
                        assessment_digest,
                        stored.external_tally,
                        stored.external_incomplete,
                    )
                    .ok_or(FileLedgerError::Corrupt)?,
                )
            }
            None => None,
        };
        let run = self.run.materialize()?;
        let gate_commit = self
            .gate_commit
            .as_ref()
            .and_then(|commit| Oid::new(run.object_format, commit.clone()))
            .ok_or(FileLedgerError::Corrupt)?;
        Ok(Publication {
            provider_run: self.provider_run.materialize()?,
            evaluation_id: ControllerEvaluationId::new(self.evaluation_id.clone())
                .ok_or(FileLedgerError::Corrupt)?,
            check: materialize_check(&self.check)?,
            run,
            gate_commit,
            conclusion: self.conclusion.materialize(),
            report: None,
            artifact,
        })
    }

    pub(super) fn validate_binding(
        &self,
        expected_evaluation_id: &str,
        delivery: &AuthenticatedDelivery,
        expected_check: &CheckBinding,
    ) -> Result<(), FileLedgerError> {
        if self.artifact.is_some() && self.report().is_none() {
            return Err(FileLedgerError::Corrupt);
        }
        if self.has_gate_commit() {
            self.materialize_metadata()?;
        } else {
            if self.artifact.is_some() {
                return Err(FileLedgerError::Corrupt);
            }
            if let Some(reference) = self.report() {
                reference.validate()?;
            }
        }
        let provider_run = self.provider_run.materialize()?;
        let evaluation_id = ControllerEvaluationId::new(self.evaluation_id.clone())
            .ok_or(FileLedgerError::Corrupt)?;
        let check = materialize_check(&self.check)?;
        let run = self.run.materialize()?;
        if self
            .gate_commit
            .as_ref()
            .is_some_and(|commit| Oid::new(run.object_format, commit.clone()).is_none())
            || evaluation_id.as_str() != expected_evaluation_id
            || provider_run != delivery.provider_run
            || run.change != delivery.change
            || run.object_format != delivery.provider_run.object_format
            || run.commits.candidate != delivery.provider_run.candidate_commit
            || check != *expected_check
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredArtifact {
    id: String,
    locator: String,
    expires_at_unix_millis: i64,
    report_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assessment_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    external_tally: Option<ExternalTally>,
    #[serde(default)]
    external_incomplete: bool,
}

impl StoredArtifact {
    fn new(reference: &ArtifactReference) -> Self {
        Self {
            id: reference.id.clone(),
            locator: reference.locator.clone(),
            expires_at_unix_millis: reference.expires_at_unix_millis,
            report_digest: reference.report_digest.to_string(),
            assessment_digest: reference.assessment_digest.map(|digest| digest.to_string()),
            external_tally: reference.external_tally,
            external_incomplete: reference.external_incomplete,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "report", rename_all = "kebab-case", deny_unknown_fields)]
enum StoredReport {
    Absent,
    Blob { reference: ReportRef },
}

impl StoredReport {
    fn attach(
        &self,
        mut publication: Publication,
        report: Option<Vec<u8>>,
    ) -> Result<Publication, FileLedgerError> {
        self.verify(report.as_deref())?;
        publication.report = report;
        Ok(publication)
    }

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger) struct ReportRef {
    digest: String,
    length: u64,
}

impl ReportRef {
    pub(in crate::file_ledger) fn new(report: &[u8]) -> Result<Self, FileLedgerError> {
        let length = report_length(report.len())?;
        Ok(Self {
            digest: hb(REPORT_DOMAIN, report).to_string(),
            length,
        })
    }

    pub(in crate::file_ledger) fn digest(&self) -> &str {
        &self.digest
    }

    pub(in crate::file_ledger) fn matches(&self, report: &[u8]) -> bool {
        u64::try_from(report.len()).ok() == Some(self.length)
            && hb(REPORT_DOMAIN, report).to_string() == self.digest
    }

    fn validate(&self) -> Result<(), FileLedgerError> {
        if self.length > MACHINE_JSON_BYTES || Digest::from_wire(&self.digest).is_none() {
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
