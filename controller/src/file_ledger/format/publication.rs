mod tests;

use std::sync::Arc;

use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::Oid;
use amiss_wire::report::MACHINE_JSON_BYTES;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactReference, AuthenticatedDelivery, CheckBinding, ControllerEvaluationId, ExternalTally,
    Publication, RunIdentity,
};

use super::model::{
    StoredChange, StoredCheck, StoredConclusion, StoredProviderRun, StoredRun, materialize_check,
    store_check,
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
    pub(in crate::file_ledger) gate_commit: Option<Oid>,
    conclusion: StoredConclusion,
    report: StoredReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact: Option<StoredArtifact>,
}

impl StoredPublication {
    pub(in crate::file_ledger) fn new(publication: &Publication) -> Result<Self, FileLedgerError> {
        validate_artifact_report(publication)?;
        let report = StoredReport::new(publication.report.as_deref())?;
        Ok(Self {
            provider_run: StoredProviderRun {
                run_id: publication.provider_run.run_id.as_str().to_owned(),
                attempt: publication.provider_run.attempt.get(),
                object_format: publication.provider_run.object_format,
                candidate_commit: publication.provider_run.candidate_commit.clone(),
            },
            evaluation_id: publication.evaluation_id.as_str().to_owned(),
            check: store_check(&publication.check),
            run: StoredRun {
                change: StoredChange::new(&publication.run.change),
                refs: publication.run.refs.clone(),
                object_format: publication.run.object_format,
                commits: publication.run.commits.clone(),
                trees: publication.run.trees.clone(),
            },
            gate_commit: Some(publication.gate_commit.clone()),
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

    pub(in crate::file_ledger) fn materialize(
        &self,
        report: Option<Vec<u8>>,
    ) -> Result<Publication, FileLedgerError> {
        let metadata = self.materialize_metadata()?;
        let gate_commit = metadata.gate_commit.ok_or(FileLedgerError::Corrupt)?;
        self.report.verify(report.as_deref())?;
        let report = report
            .map(|bytes| {
                amiss_wire::report::validate_envelope(&bytes)
                    .map(|(envelope, _verdict)| Arc::new(crate::CapturedReport { bytes, envelope }))
            })
            .transpose()
            .map_err(|_defect| FileLedgerError::Corrupt)?;
        let publication = Publication {
            provider_run: metadata.provider_run,
            evaluation_id: metadata.evaluation_id,
            check: metadata.check,
            run: metadata.run,
            gate_commit,
            conclusion: metadata.conclusion,
            report,
            artifact: metadata.artifact,
        };
        validate_artifact_report(&publication)?;
        Ok(publication)
    }

    pub(super) fn materialize_metadata(&self) -> Result<Publication<Option<Oid>>, FileLedgerError> {
        if let Some(reference) = self.report() {
            reference.validate()?;
        }
        let artifact = match &self.artifact {
            Some(stored) => {
                let assessment_digest = stored
                    .assessment_digest
                    .as_deref()
                    .map(|raw| Digest::from_wire(raw).ok_or(FileLedgerError::Corrupt))
                    .transpose()?;
                let semantic_digest = stored
                    .semantic_digest
                    .as_deref()
                    .map(|raw| Digest::from_wire(raw).ok_or(FileLedgerError::Corrupt))
                    .transpose()?;
                Some(
                    crate::artifacts::checked_reference(ArtifactReference {
                        id: stored.id.clone(),
                        locator: stored.locator.clone(),
                        expires_at_unix_millis: stored.expires_at_unix_millis,
                        report_digest: Digest::from_wire(&stored.report_digest)
                            .ok_or(FileLedgerError::Corrupt)?,
                        semantic_digest,
                        assessment_digest,
                        external_tally: stored.external_tally,
                        external_incomplete: stored.external_incomplete,
                    })
                    .ok_or(FileLedgerError::Corrupt)?,
                )
            }
            None => None,
        };
        let run = RunIdentity::new(
            self.run.change.materialize()?,
            self.run.refs.clone(),
            self.run.object_format,
            self.run.commits.clone(),
            self.run.trees.clone(),
        )
        .ok_or(FileLedgerError::Corrupt)?;
        if self
            .gate_commit
            .as_ref()
            .is_some_and(|commit| commit.object_format() != run.object_format)
        {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(Publication {
            provider_run: self.provider_run.materialize()?,
            evaluation_id: ControllerEvaluationId::new(self.evaluation_id.clone())
                .ok_or(FileLedgerError::Corrupt)?,
            check: materialize_check(&self.check)?,
            run,
            gate_commit: self.gate_commit.clone(),
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
        if self.artifact.is_some() && (self.report().is_none() || self.gate_commit.is_none()) {
            return Err(FileLedgerError::Corrupt);
        }
        let publication = self.materialize_metadata()?;
        if publication.evaluation_id.as_str() != expected_evaluation_id
            || publication.provider_run != delivery.provider_run
            || publication.run.change != delivery.change
            || publication.run.object_format != delivery.provider_run.object_format
            || publication.run.commits.candidate != delivery.provider_run.candidate_commit
            || publication.check != *expected_check
        {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(())
    }
}

fn validate_artifact_report(publication: &Publication) -> Result<(), FileLedgerError> {
    match &publication.artifact {
        Some(reference)
            if !crate::artifacts::reference_matches_report(
                reference,
                publication
                    .report
                    .as_ref()
                    .map(|report| report.bytes.as_slice()),
            ) =>
        {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic_digest: Option<String>,
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
            semantic_digest: reference.semantic_digest.map(|digest| digest.to_string()),
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
    fn new(report: Option<&crate::CapturedReport>) -> Result<Self, FileLedgerError> {
        match report {
            Some(report) => ReportRef::new(&report.bytes).map(|reference| Self::Blob { reference }),
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
