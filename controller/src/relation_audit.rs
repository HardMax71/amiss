use amiss_wire::digest::{Digest, sha256};
use amiss_wire::relation::{
    RELATION_DOCUMENT_BYTES, RelationVerdict, assess, parse_assessment, parse_evidence, parse_plan,
};

use crate::audit_report::accepted_report;
use crate::{ArtifactError, RelationTransition, verify_relation_plan};

#[derive(Clone, Copy)]
pub struct RelationAuditBundle<'a> {
    pub transition: &'a RelationTransition,
    pub report: &'a [u8],
    pub plan: &'a [u8],
    pub evidence: Option<&'a [u8]>,
    pub assessment: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationAuditDigests {
    pub report_digest: Digest,
    pub plan_digest: Digest,
    pub evidence_digest: Option<Digest>,
    pub assessment_digest: Digest,
    pub verdict: RelationVerdict,
}

/// Validates one complete relation audit against its accepted trigger report
/// and frozen operator transition before retention.
///
/// No repository or provider material is acquired or interpreted here.
///
/// # Errors
///
/// Returns [`ArtifactError::TooLarge`] when a component crosses its contract
/// ceiling and [`ArtifactError::Corrupt`] for every malformed, substituted,
/// or non-replayable chain.
pub fn validate_relation_audit(
    bundle: RelationAuditBundle<'_>,
) -> Result<RelationAuditDigests, ArtifactError> {
    if [bundle.plan, bundle.assessment]
        .into_iter()
        .chain(bundle.evidence)
        .any(|bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX) > RELATION_DOCUMENT_BYTES)
    {
        return Err(ArtifactError::TooLarge);
    }
    let report = accepted_report(bundle.report)?;
    let plan = parse_plan(bundle.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    verify_relation_plan(&plan, bundle.transition).map_err(|_defect| ArtifactError::Corrupt)?;
    let trigger = plan
        .payload
        .subjects
        .iter()
        .find(|subject| subject.role == plan.payload.trigger_role)
        .ok_or(ArtifactError::Corrupt)?;
    if plan.payload.report_payload_digest != report.payload_digest
        || trigger.repository != report.repository
        || report.target_ref.as_ref() != Some(&trigger.target)
        || trigger.base.commit != report.base.commit
        || trigger.base.tree != report.base.tree
        || trigger.candidate.commit != report.candidate.commit
        || trigger.candidate.tree != report.candidate.tree
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
        &assessment.payload.engine_version,
        assessment.payload.engine_digest,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    if replayed.text("payload_digest") != Some(&assessment.payload_digest.to_string()) {
        return Err(ArtifactError::Corrupt);
    }
    Ok(RelationAuditDigests {
        report_digest: report.report_digest,
        plan_digest: sha256(bundle.plan),
        evidence_digest: bundle.evidence.map(sha256),
        assessment_digest: sha256(bundle.assessment),
        verdict: assessment.payload.verdict,
    })
}
