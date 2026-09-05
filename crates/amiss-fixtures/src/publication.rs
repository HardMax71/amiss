use amiss_wire::digest::{Digest, sha256};
use amiss_wire::json;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::publication::{
    DocsCandidate, PublicationEvidenceEnvelope, assess, evidence, parse_evidence, parse_plan, plan,
};

const REPORT: &[u8] = include_bytes!("../../../spec/examples/scanner-report.json");
const PLAN: &[u8] = include_bytes!("../../../spec/examples/publication-plan.json");
const EVIDENCE: &[u8] = include_bytes!("../../../spec/examples/publication-evidence.json");

pub struct PublicationAuditFixture {
    pub report: Vec<u8>,
    pub plan: Vec<u8>,
    pub evidence: Option<Vec<u8>>,
    pub assessment: Vec<u8>,
}

/// Builds one exact report-bound publication audit for controller tests.
#[must_use]
pub fn publication_audit(with_evidence: bool) -> Option<PublicationAuditFixture> {
    let report = REPORT.to_vec();
    let parsed = json::parse(&report).ok()?;
    let (_, report_payload_digest, _) = amiss_wire::report::validate_envelope(&parsed).ok()?;
    let mut plan_envelope = parse_plan(PLAN).ok()?;
    plan_envelope.payload.report_payload_digest = report_payload_digest;
    plan_envelope.payload.docs = DocsCandidate {
        repository: RepositoryIdentity::new(
            "git.example.internal".to_owned(),
            "group/subgroup".to_owned(),
            "widget".to_owned(),
        )?,
        object_format: ObjectFormat::Sha1,
        commit: Oid::new(
            ObjectFormat::Sha1,
            "d1a175a1986230e4ba44b6f6ed67c8dbccb29aaf".to_owned(),
        )?,
        tree: Oid::new(
            ObjectFormat::Sha1,
            "7eed0bc378155f11543b2261997a1f363557e8cd".to_owned(),
        )?,
        candidate_identity_digest: Digest::from_wire(
            "sha256:8c8f4c8087edf216675ffbfc5a75a6c67dc48103be696b74174758a3e5db187a",
        )?,
    };
    let plan_bytes = plan(&plan_envelope.payload).ok()?;
    let plan_envelope = parse_plan(&plan_bytes).ok()?;
    let evidence_envelope = if with_evidence {
        Some(publication_evidence(&plan_envelope)?)
    } else {
        None
    };
    let assessment = assess(
        &plan_envelope,
        evidence_envelope.as_ref(),
        env!("CARGO_PKG_VERSION"),
        sha256(b"publication evaluator fixture"),
    )
    .ok()?;
    let evidence = evidence_envelope
        .as_ref()
        .map(|envelope| evidence(&envelope.payload))
        .transpose()
        .ok()?;
    Some(PublicationAuditFixture {
        report,
        plan: plan_bytes,
        evidence,
        assessment,
    })
}

fn publication_evidence(
    plan: &amiss_wire::publication::PublicationPlanEnvelope,
) -> Option<PublicationEvidenceEnvelope> {
    let mut evidence_envelope = parse_evidence(EVIDENCE).ok()?;
    evidence_envelope.payload.plan_payload_digest = plan.payload_digest;
    evidence_envelope.payload.producer = plan.payload.producer.clone();
    evidence_envelope.payload.docs = plan.payload.docs.clone();
    evidence_envelope.payload.target = plan.payload.target.clone();
    evidence_envelope.payload.site = plan.payload.site.clone();
    evidence_envelope.payload.product = plan.payload.product.clone();
    let value = evidence(&evidence_envelope.payload).ok()?;
    parse_evidence(&value).ok()
}
