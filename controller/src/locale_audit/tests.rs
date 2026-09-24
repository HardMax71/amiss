#![cfg(test)]

use amiss_fixtures::{LocaleAuditFixture, locale_audit};
use amiss_wire::assessment::AssessmentVerdict;
use amiss_wire::envelope::Payload as _;
use amiss_wire::locale::{
    ASSESSMENT_DOCUMENT_BYTES, LOCALE_DOCUMENT_BYTES, LocaleCoverageEvidence, LocaleCoveragePlan,
    assess,
};
use amiss_wire::model::Digest;
use sha2::Digest as _;

use super::{LocaleAuditBundle, validate_locale_audit};
use crate::ArtifactError;

#[test]
fn one_exact_chain_binds_every_retained_byte_to_the_report() -> Result<(), ArtifactError> {
    let fixture = locale_audit(true).ok_or(ArtifactError::Corrupt)?;
    let audit = validate_locale_audit(bundle(&fixture))?;

    assert_eq!(
        audit.report_digest,
        Digest::from(sha2::Sha256::digest(&fixture.report).0)
    );
    assert_eq!(
        audit.plan_digest,
        Digest::from(sha2::Sha256::digest(&fixture.plan).0)
    );
    assert_eq!(
        audit.evidence_digest,
        fixture
            .evidence
            .as_deref()
            .map(|bytes| Digest::from(sha2::Sha256::digest(bytes).0))
    );
    assert_eq!(
        audit.assessment_digest,
        Digest::from(sha2::Sha256::digest(&fixture.assessment).0)
    );
    assert_eq!(audit.verdict, AssessmentVerdict::Matched);
    Ok(())
}

#[test]
fn absent_evidence_remains_a_replayable_unproven_audit() -> Result<(), ArtifactError> {
    let fixture = locale_audit(false).ok_or(ArtifactError::Corrupt)?;
    let audit = validate_locale_audit(bundle(&fixture))?;

    assert_eq!(audit.evidence_digest, None);
    assert_eq!(audit.verdict, AssessmentVerdict::Unproven);
    Ok(())
}

#[test]
fn a_plan_bound_to_another_report_is_refused() -> Result<(), ArtifactError> {
    let fixture = locale_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut plan =
        LocaleCoveragePlan::parse(&fixture.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.report_payload_digest = Digest::from([9; 32]);
    let rebound = rebuilt(&fixture, &plan.payload)?;

    assert!(matches!(
        validate_locale_audit(LocaleAuditBundle {
            plan: &rebound.plan,
            evidence: rebound.evidence.as_deref(),
            assessment: &rebound.assessment,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn a_foreign_assessment_does_not_replay() -> Result<(), ArtifactError> {
    let fixture = locale_audit(true).ok_or(ArtifactError::Corrupt)?;
    let absent = locale_audit(false).ok_or(ArtifactError::Corrupt)?;

    assert!(matches!(
        validate_locale_audit(LocaleAuditBundle {
            assessment: &absent.assessment,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn each_component_is_bounded_by_its_own_contract_ceiling() -> Result<(), ArtifactError> {
    let fixture = locale_audit(true).ok_or(ArtifactError::Corrupt)?;
    let oversized_plan =
        vec![b'x'; usize::try_from(LOCALE_DOCUMENT_BYTES).unwrap_or(usize::MAX) + 1];
    assert!(matches!(
        validate_locale_audit(LocaleAuditBundle {
            plan: &oversized_plan,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::TooLarge)
    ));

    // The plan ceiling is 64 KiB while evidence and the assessment reach 16 MiB,
    // so a plan-sized evidence document is a parse defect, never a size refusal.
    let evidence = vec![b'x'; usize::try_from(LOCALE_DOCUMENT_BYTES).unwrap_or(usize::MAX) + 1];
    assert!(matches!(
        validate_locale_audit(LocaleAuditBundle {
            evidence: Some(&evidence),
            ..bundle(&fixture)
        }),
        Err(ArtifactError::Corrupt)
    ));
    const { assert!(ASSESSMENT_DOCUMENT_BYTES > LOCALE_DOCUMENT_BYTES) };
    Ok(())
}

fn rebuilt(
    fixture: &LocaleAuditFixture,
    plan: &LocaleCoveragePlan,
) -> Result<LocaleAuditFixture, ArtifactError> {
    let plan_bytes = plan.emit().map_err(|_defect| ArtifactError::Corrupt)?;
    let plan = LocaleCoveragePlan::parse(&plan_bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let mut evidence =
        LocaleCoverageEvidence::parse(fixture.evidence.as_deref().ok_or(ArtifactError::Corrupt)?)
            .map_err(|_defect| ArtifactError::Corrupt)?;
    evidence.payload.plan_payload_digest = plan.payload_digest;
    let evidence_bytes = evidence
        .payload
        .emit()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let evidence =
        LocaleCoverageEvidence::parse(&evidence_bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment = assess(
        &plan,
        Some(&evidence),
        env!("CARGO_PKG_VERSION"),
        Digest::from([32; 32]),
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    Ok(LocaleAuditFixture {
        report: fixture.report.clone(),
        plan: plan_bytes,
        evidence: Some(evidence_bytes),
        assessment,
    })
}

fn bundle(fixture: &LocaleAuditFixture) -> LocaleAuditBundle<'_> {
    LocaleAuditBundle {
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}
