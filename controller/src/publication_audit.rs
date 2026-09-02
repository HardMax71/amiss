mod tests;

use amiss_wire::digest::{Digest, sha256};
use amiss_wire::publication::{
    DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationVerdict, assess, parse_assessment,
    parse_evidence, parse_plan,
};

use crate::ArtifactError;
use crate::audit_report::accepted_report;

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
    pub verdict: PublicationVerdict,
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
    let report_docs = DocsCandidate {
        repository: report.repository,
        object_format: report.candidate.commit.object_format(),
        commit: report.candidate.commit,
        tree: report.candidate.tree,
        candidate_identity_digest: report.candidate_identity_digest,
    };
    let plan = parse_plan(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    if plan.payload.report_payload_digest != report.payload_digest
        || plan.payload.docs != report_docs
    {
        return Err(ArtifactError::Corrupt);
    }
    let evidence = bundle
        .evidence
        .map(parse_evidence)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment =
        parse_assessment(bundle.assessment).map_err(|_defect| ArtifactError::Corrupt)?;
    let replayed = assess(
        &plan,
        evidence.as_ref(),
        &assessment.payload.engine.engine_version,
        assessment.payload.engine.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    if replayed.text("payload_digest") != Some(&assessment.payload_digest.to_string()) {
        return Err(ArtifactError::Corrupt);
    }
    Ok(PublicationAuditDigests {
        report_digest: report.report_digest,
        plan_digest: sha256(bundle.plan),
        evidence_digest: bundle.evidence.map(sha256),
        assessment_digest: sha256(bundle.assessment),
        verdict: assessment.payload.verdict,
    })
}
