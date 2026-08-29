#![cfg(test)]

use amiss_wire::digest::{Digest, sha256};
use amiss_wire::json::{self, Value};
use amiss_wire::publication::{
    PublicationEvidenceEnvelope, PublicationPlanEnvelope, PublicationVerdict, assess,
    evidence as write_evidence, parse_evidence, parse_plan, plan as write_plan,
};

use super::{PublicationAuditBundle, report_docs, validate_publication_audit};
use crate::ArtifactError;

const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.json");
const PLAN: &[u8] = include_bytes!("../../../spec/examples/publication-plan.json");
const EVIDENCE: &[u8] = include_bytes!("../../../spec/examples/publication-evidence.json");

struct Fixture {
    plan: Vec<u8>,
    evidence: Option<Vec<u8>>,
    assessment: Vec<u8>,
}

#[test]
fn one_exact_chain_binds_every_retained_byte_to_the_report() -> Result<(), ArtifactError> {
    let fixture = fixture(true)?;
    let audit = validate_publication_audit(bundle(&fixture))?;

    assert_eq!(audit.report_digest, sha256(REPORT));
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
    let parsed = json::parse(REPORT).map_err(|_defect| ArtifactError::Corrupt)?;
    let payload = parsed.member("payload").ok_or(ArtifactError::Corrupt)?;
    let docs = report_docs(payload.clone()).ok_or(ArtifactError::Corrupt)?;
    let evaluation = payload.member("evaluation").ok_or(ArtifactError::Corrupt)?;
    let candidate = evaluation
        .member("candidate")
        .ok_or(ArtifactError::Corrupt)?;
    assert_eq!(
        docs.candidate_identity_digest,
        Digest::from_wire(
            "sha256:8c8f4c8087edf216675ffbfc5a75a6c67dc48103be696b74174758a3e5db187a"
        )
        .ok_or(ArtifactError::Corrupt)?
    );
    assert_eq!(candidate.text("commit_oid"), Some(docs.commit.as_str()));
    Ok(())
}

#[test]
fn absent_evidence_remains_a_replayable_unproven_audit() -> Result<(), ArtifactError> {
    let fixture = fixture(false)?;
    let audit = validate_publication_audit(bundle(&fixture))?;

    assert_eq!(audit.evidence_digest, None);
    assert_eq!(audit.verdict, PublicationVerdict::Unproven);
    Ok(())
}

#[test]
fn report_plan_and_assessment_rebindings_are_refused() -> Result<(), ArtifactError> {
    let mut wrong_report = fixture(true)?;
    let mut plan = parse_plan(&wrong_report.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.report_payload_digest = Digest::from_wire(
        "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    )
    .ok_or(ArtifactError::Corrupt)?;
    wrong_report = rebuilt(&plan, wrong_report.evidence.as_deref())?;
    assert!(matches!(
        validate_publication_audit(bundle(&wrong_report)),
        Err(ArtifactError::Corrupt)
    ));

    let mut wrong_docs = fixture(true)?;
    let mut plan = parse_plan(&wrong_docs.plan).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.docs.commit = plan.payload.docs.tree.clone();
    wrong_docs = rebuilt(&plan, wrong_docs.evidence.as_deref())?;
    assert!(matches!(
        validate_publication_audit(bundle(&wrong_docs)),
        Err(ArtifactError::Corrupt)
    ));

    let exact = fixture(true)?;
    let other = fixture(false)?;
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
    let fixture = fixture(true)?;
    let mut report = json::parse(REPORT).map_err(|_defect| ArtifactError::Corrupt)?;
    let Value::Object(envelope) = &mut report else {
        return Err(ArtifactError::Corrupt);
    };
    let payload = envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    let Value::Object(payload_members) = payload else {
        return Err(ArtifactError::Corrupt);
    };
    let result = payload_members
        .iter_mut()
        .find_map(|(key, value)| (key == "result").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    *result = Value::object(vec![
        ("complete".to_owned(), Value::Bool(false)),
        ("exit_code".to_owned(), Value::Integer(2)),
        ("status".to_owned(), Value::string("incomplete".to_owned())),
    ]);
    let digest = amiss_wire::digest::hj(amiss_wire::report::PAYLOAD_SCHEMA, payload);
    let digest_value = envelope
        .iter_mut()
        .find_map(|(key, value)| (key == "payload_digest").then_some(value))
        .ok_or(ArtifactError::Corrupt)?;
    *digest_value = Value::string(digest.to_string());
    let incomplete = json::canonical(&report);
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

fn fixture(with_evidence: bool) -> Result<Fixture, ArtifactError> {
    let report = json::parse(REPORT).map_err(|_defect| ArtifactError::Corrupt)?;
    let (payload, report_digest, _verdict) =
        amiss_wire::report::validate_envelope(&report).map_err(|_defect| ArtifactError::Corrupt)?;
    let mut plan = parse_plan(PLAN).map_err(|_defect| ArtifactError::Corrupt)?;
    plan.payload.report_payload_digest =
        Digest::from_wire(report_digest).ok_or(ArtifactError::Corrupt)?;
    plan.payload.docs = report_docs(payload.clone()).ok_or(ArtifactError::Corrupt)?;
    let plan_value = write_plan(&plan.payload).map_err(|_defect| ArtifactError::Corrupt)?;
    let plan_bytes = json::canonical(&plan_value);
    let plan = parse_plan(&plan_bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let evidence = with_evidence.then(|| evidence_for(&plan)).transpose()?;
    let assessment = assess(
        &plan,
        evidence.as_ref(),
        "0.26.0",
        sha256(b"publication evaluator"),
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let evidence_bytes = evidence
        .as_ref()
        .map(|envelope| {
            write_evidence(&envelope.payload)
                .map(|value| json::canonical(&value))
                .map_err(|_defect| ArtifactError::Corrupt)
        })
        .transpose()?;
    Ok(Fixture {
        plan: plan_bytes,
        evidence: evidence_bytes,
        assessment: json::canonical(&assessment),
    })
}

fn evidence_for(
    plan: &PublicationPlanEnvelope,
) -> Result<PublicationEvidenceEnvelope, ArtifactError> {
    let mut evidence = parse_evidence(EVIDENCE).map_err(|_defect| ArtifactError::Corrupt)?;
    evidence.payload.plan_payload_digest = plan.payload_digest;
    evidence.payload.producer = plan.payload.producer.clone();
    evidence.payload.docs = plan.payload.docs.clone();
    evidence.payload.target = plan.payload.target.clone();
    evidence.payload.site = plan.payload.site.clone();
    evidence.payload.product = plan.payload.product.clone();
    let value = write_evidence(&evidence.payload).map_err(|_defect| ArtifactError::Corrupt)?;
    parse_evidence(&json::canonical(&value)).map_err(|_defect| ArtifactError::Corrupt)
}

fn rebuilt(
    plan: &PublicationPlanEnvelope,
    evidence_bytes: Option<&[u8]>,
) -> Result<Fixture, ArtifactError> {
    let plan_value = write_plan(&plan.payload).map_err(|_defect| ArtifactError::Corrupt)?;
    let plan_bytes = json::canonical(&plan_value);
    let plan = parse_plan(&plan_bytes).map_err(|_defect| ArtifactError::Corrupt)?;
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
    Ok(Fixture {
        plan: plan_bytes,
        evidence,
        assessment: json::canonical(&assessment),
    })
}

fn bundle(fixture: &Fixture) -> PublicationAuditBundle<'_> {
    PublicationAuditBundle {
        report: REPORT,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}
