mod tests;

use amiss_wire::assessment::AssessmentVerdict;
use amiss_wire::envelope::Payload as _;
use amiss_wire::model::Digest;
use amiss_wire::publication::{
    PUBLICATION_DOCUMENT_BYTES, PublicationAssessment, PublicationEvidence, PublicationPlan,
};

use crate::ArtifactError;
use crate::audit_report::{accepted_docs, accepted_report, component_digests};

#[derive(Clone, Copy)]
pub struct PublicationAuditBundle<'a> {
    pub report: &'a [u8],
    pub plan: &'a [u8],
    pub evidence: Option<&'a [u8]>,
    pub assessment: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicationAuditDigests {
    pub report_digest: Digest,
    pub plan_digest: Digest,
    pub evidence_digest: Option<Digest>,
    pub assessment_digest: Digest,
    pub verdict: AssessmentVerdict,
}

/// Validates one complete, report-bound publication audit before retention.
///
/// The plan must describe the repository candidate in the accepted report,
/// and the assessment must replay exactly from the supplied plan and optional
/// evidence. No provider material is acquired or interpreted here.
///
/// # Errors
///
/// Returns [`ArtifactError::TooLarge`] when a component crosses its contract
/// ceiling and [`ArtifactError::Corrupt`] for every malformed or inconsistent
/// chain.
pub fn validate_publication_audit(
    bundle: PublicationAuditBundle<'_>,
) -> Result<PublicationAuditDigests, ArtifactError> {
    if [bundle.plan, bundle.assessment]
        .into_iter()
        .chain(bundle.evidence)
        .any(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES)
    {
        return Err(ArtifactError::TooLarge);
    }
    let report = accepted_report(bundle.report)?;
    let plan = PublicationPlan::parse(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    if plan.payload.report_payload_digest != report.payload_digest
        || plan.payload.docs != accepted_docs(&report)
    {
        return Err(ArtifactError::Corrupt);
    }
    let evidence = bundle
        .evidence
        .map(PublicationEvidence::parse)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment = PublicationAssessment::parse(bundle.assessment)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = PublicationAssessment::evaluate(
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
    Ok(PublicationAuditDigests {
        report_digest: report.report_digest,
        plan_digest,
        evidence_digest,
        assessment_digest,
        verdict: assessment.payload.verdict,
    })
}
