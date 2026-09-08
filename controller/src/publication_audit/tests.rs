#![cfg(test)]

use amiss_fixtures::{PublicationAuditFixture, publication_audit};
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::publication::{
    PublicationPlan, PublicationVerdict, assess, parse_evidence, parse_plan, plan,
};
use amiss_wire::report::model::{Evaluation, ReportEnvelope, ReportStatus, Snapshot};
use amiss_wire::requests::CandidateSnapshot;

use super::{PublicationAuditBundle, validate_publication_audit};
use crate::ArtifactError;
use crate::audit_report::accepted_report;

#[test]
fn one_exact_chain_binds_every_retained_byte_to_the_report() -> Result<(), ArtifactError> {
    let fixture = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let audit = validate_publication_audit(bundle(&fixture))?;

    assert_eq!(audit.report_digest, sha256(&fixture.report));
    assert_eq!(audit.plan_digest, sha256(&fixture.plan));
    assert_eq!(
        audit.evidence_digest,
        fixture.evidence.as_deref().map(sha256)
    );
    assert_eq!(audit.assessment_digest, sha256(&fixture.assessment));
    assert_eq!(audit.verdict, PublicationVerdict::Matched);
    Ok(())
}

#[test]
fn the_reported_candidate_identity_has_the_published_preimage() -> Result<(), ArtifactError> {
    let fixture = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let report = accepted_report(&fixture.report)?;
    let parsed: ReportEnvelope =
        serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    let Evaluation::Resolved(evaluation) = parsed.payload.evaluation else {
        return Err(ArtifactError::Corrupt);
    };
    let Snapshot::Available(CandidateSnapshot::Git(candidate)) = evaluation.candidate else {
        return Err(ArtifactError::Corrupt);
    };
    assert_eq!(
        report.candidate_identity_digest,
        Digest::from_wire(
            "sha256:8c8f4c8087edf216675ffbfc5a75a6c67dc48103be696b74174758a3e5db187a"
        )
        .ok_or(ArtifactError::Corrupt)?
    );
    assert_eq!(candidate.commit_oid, report.candidate.commit);
    Ok(())
}

#[test]
fn null_target_is_distinct_from_an_absent_target_key() -> Result<(), ArtifactError> {
    let fixture = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    assert_eq!(accepted_report(&fixture.report)?.target_ref, None);

    let mut report: ReportEnvelope =
        serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    assert_eq!(payload.matches(r#""target_ref":null"#).count(), 1);
    let missing = payload.replacen(r#","target_ref":null"#, "", 1);
    let payload_digest =
        amiss_wire::digest::hb(amiss_wire::report::PAYLOAD_SCHEMA, missing.as_bytes());
    assert_ne!(missing, payload);
    report.payload_digest = payload_digest;
    let document = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
    let missing = document.replacen(&payload, &missing, 1);
    assert_ne!(missing, document);

    assert!(matches!(
        accepted_report(missing.as_bytes()),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn absent_evidence_remains_a_replayable_unproven_audit() -> Result<(), ArtifactError> {
    let fixture = publication_audit(false).ok_or(ArtifactError::Corrupt)?;
    let audit = validate_publication_audit(bundle(&fixture))?;

    assert_eq!(audit.evidence_digest, None);
    assert_eq!(audit.verdict, PublicationVerdict::Unproven);
    Ok(())
}

#[test]
fn report_plan_and_assessment_rebindings_are_refused() -> Result<(), ArtifactError> {
    let mut wrong_report = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut plan = parse_plan(&wrong_report.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.report_payload_digest = Digest::from_wire(
        "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    )
    .ok_or(ArtifactError::Corrupt)?;
    wrong_report = rebuilt(
        &wrong_report,
        plan.payload,
        wrong_report.evidence.as_deref(),
    )?;
    assert!(matches!(
        validate_publication_audit(bundle(&wrong_report)),
        Err(ArtifactError::Corrupt)
    ));

    let mut wrong_docs = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut plan = parse_plan(&wrong_docs.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.docs.commit = plan.payload.docs.tree.clone();
    wrong_docs = rebuilt(&wrong_docs, plan.payload, wrong_docs.evidence.as_deref())?;
    assert!(matches!(
        validate_publication_audit(bundle(&wrong_docs)),
        Err(ArtifactError::Corrupt)
    ));

    let exact = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let other = publication_audit(false).ok_or(ArtifactError::Corrupt)?;
    assert!(matches!(
        validate_publication_audit(PublicationAuditBundle {
            assessment: &other.assessment,
            ..bundle(&exact)
        }),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}

#[test]
fn incomplete_reports_and_oversized_publication_documents_are_refused() -> Result<(), ArtifactError>
{
    let fixture = publication_audit(true).ok_or(ArtifactError::Corrupt)?;
    let mut report: ReportEnvelope =
        serde_json::from_slice(&fixture.report).map_err(|_defect| ArtifactError::Corrupt)?;
    report.payload.result.complete = false;
    report.payload.result.exit_code = 2;
    report.payload.result.status = ReportStatus::Incomplete;
    report.payload_digest = amiss_wire::digest::hb(
        amiss_wire::report::PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    let incomplete = serde_json_canonicalizer::to_vec(&report).unwrap();
    assert_eq!(
        amiss_wire::report::validate_envelope(&incomplete)
            .unwrap()
            .1,
        amiss_wire::ExitClass::Failure
    );
    assert!(matches!(
        accepted_report(&incomplete),
        Err(ArtifactError::Corrupt)
    ));
    assert!(matches!(
        validate_publication_audit(PublicationAuditBundle {
            report: &incomplete,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::Corrupt)
    ));

    let oversized = vec![
        b' ';
        usize::try_from(amiss_wire::publication::PUBLICATION_DOCUMENT_BYTES)
            .map_err(|_defect| ArtifactError::Corrupt)?
            .saturating_add(1)
    ];
    assert!(matches!(
        validate_publication_audit(PublicationAuditBundle {
            plan: &oversized,
            ..bundle(&fixture)
        }),
        Err(ArtifactError::TooLarge)
    ));
    Ok(())
}

fn rebuilt(
    fixture: &PublicationAuditFixture,
    input: PublicationPlan,
    evidence_bytes: Option<&[u8]>,
) -> Result<PublicationAuditFixture, ArtifactError> {
    let plan = plan(input).map_err(|_defect| ArtifactError::Corrupt)?;
    let evidence = evidence_bytes.map(<[u8]>::to_vec);
    let parsed_evidence = evidence
        .as_deref()
        .map(parse_evidence)
        .transpose()
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let assessment = assess(
        &plan,
        parsed_evidence.as_ref(),
        "0.26.0",
        sha256(b"publication evaluator"),
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let mut plan_bytes = Vec::new();
    amiss_wire::write_json(
        &plan,
        &mut plan_bytes,
        amiss_wire::publication::PUBLICATION_DOCUMENT_BYTES,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let mut assessment_bytes = Vec::new();
    amiss_wire::write_json(
        &assessment,
        &mut assessment_bytes,
        amiss_wire::publication::PUBLICATION_DOCUMENT_BYTES,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    Ok(PublicationAuditFixture {
        report: fixture.report.clone(),
        plan: plan_bytes,
        evidence,
        assessment: assessment_bytes,
    })
}

fn bundle(fixture: &PublicationAuditFixture) -> PublicationAuditBundle<'_> {
    PublicationAuditBundle {
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}
