mod tests;

use amiss_wire::assessment::AssessmentVerdict;
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    ASSESSMENT_DOCUMENT_BYTES, EVIDENCE_DOCUMENT_BYTES, LOCALE_DOCUMENT_BYTES,
    LocaleCoverageAssessment, LocaleCoverageEvidence, LocaleCoveragePlan,
};
use amiss_wire::model::Digest;

use crate::ArtifactError;
use crate::audit_report::{accepted_docs, accepted_report, component_digests};

#[derive(Clone, Copy)]
pub struct LocaleAuditBundle<'a> {
    pub report: &'a [u8],
    pub plan: &'a [u8],
    pub evidence: Option<&'a [u8]>,
    pub assessment: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocaleAuditDigests {
    pub report_digest: Digest,
    pub plan_digest: Digest,
    pub evidence_digest: Option<Digest>,
    pub assessment_digest: Digest,
    pub verdict: AssessmentVerdict,
}

/// Validates one complete, report-bound locale coverage audit before retention.
///
/// The plan must describe the repository candidate in the accepted report, and
/// the assessment must replay exactly from the supplied plan and optional
/// evidence. No generator material is acquired or interpreted here.
///
/// # Errors
///
/// Returns [`ArtifactError::TooLarge`] when a component crosses its contract
/// ceiling and [`ArtifactError::Corrupt`] for every malformed or inconsistent
/// chain.
pub fn validate_locale_audit(
    bundle: LocaleAuditBundle<'_>,
) -> Result<LocaleAuditDigests, ArtifactError> {
    let oversized =
        |bytes: &[u8], maximum| u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum;
    if oversized(bundle.plan, LOCALE_DOCUMENT_BYTES)
        || oversized(bundle.assessment, ASSESSMENT_DOCUMENT_BYTES)
        || bundle
            .evidence
            .is_some_and(|bytes| oversized(bytes, EVIDENCE_DOCUMENT_BYTES))
    {
        return Err(ArtifactError::TooLarge);
    }
    let report = accepted_report(bundle.report)?;
    let plan = LocaleCoveragePlan::parse(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    if plan.payload.report_payload_digest != report.payload_digest
        || plan.payload.docs != accepted_docs(&report)
    {
        return Err(ArtifactError::Corrupt);
    }
    let evidence = bundle
        .evidence
        .map(LocaleCoverageEvidence::parse)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment = LocaleCoverageAssessment::parse(bundle.assessment)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = LocaleCoverageAssessment::evaluate(
        &plan,
        evidence.as_ref(),
        &assessment.payload.engine.engine_version,
        assessment.payload.engine.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    if replayed != assessment.payload {
        return Err(ArtifactError::Corrupt);
    }
    let (plan_digest, evidence_digest, assessment_digest) =
        component_digests(bundle.plan, bundle.evidence, bundle.assessment);
    Ok(LocaleAuditDigests {
        report_digest: report.report_digest,
        plan_digest,
        evidence_digest,
        assessment_digest,
        verdict: assessment.payload.verdict,
    })
}
