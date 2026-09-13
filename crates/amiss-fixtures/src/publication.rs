use amiss_wire::envelope::{Envelope, Payload as _};
use amiss_wire::model::Digest;
use amiss_wire::publication::{PublicationEvidence, PublicationPlan, assess};

use crate::audit::report_binding;

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
    let binding = report_binding()?;
    let mut plan_envelope = PublicationPlan::parse(PLAN).ok()?;
    plan_envelope.payload.report_payload_digest = binding.payload_digest;
    plan_envelope.payload.docs = binding.docs;
    let plan_bytes = plan_envelope.payload.emit().ok()?;
    let plan_envelope = PublicationPlan::parse(&plan_bytes).ok()?;
    let evidence_envelope = if with_evidence {
        Some(publication_evidence(&plan_envelope)?)
    } else {
        None
    };
    let assessment = assess(
        &plan_envelope,
        evidence_envelope.as_ref(),
        env!("CARGO_PKG_VERSION"),
        Digest::from([32; 32]),
    )
    .ok()?;
    let evidence = evidence_envelope
        .as_ref()
        .map(|envelope| envelope.payload.emit())
        .transpose()
        .ok()?;
    Some(PublicationAuditFixture {
        report: binding.report,
        plan: plan_bytes,
        evidence,
        assessment,
    })
}

fn publication_evidence(plan: &Envelope<PublicationPlan>) -> Option<Envelope<PublicationEvidence>> {
    let mut evidence_envelope = PublicationEvidence::parse(EVIDENCE).ok()?;
    evidence_envelope.payload.plan_payload_digest = plan.payload_digest;
    evidence_envelope.payload.producer = plan.payload.producer.clone();
    evidence_envelope.payload.docs = plan.payload.docs.clone();
    evidence_envelope.payload.target = plan.payload.target.clone();
    evidence_envelope.payload.site = plan.payload.site.clone();
    evidence_envelope.payload.product = plan.payload.product.clone();
    let value = evidence_envelope.payload.emit().ok()?;
    PublicationEvidence::parse(&value).ok()
}
