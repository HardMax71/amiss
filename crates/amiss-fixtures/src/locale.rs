use amiss_wire::envelope::{Envelope, Payload as _};
use amiss_wire::locale::{LocaleCoverageEvidence, LocaleCoveragePlan, assess};
use amiss_wire::model::Digest;

use crate::audit::report_binding;

const PLAN: &[u8] = include_bytes!("../../../spec/examples/locale-coverage-plan.json");
const EVIDENCE: &[u8] = include_bytes!("../../../spec/examples/locale-coverage-evidence.json");

pub struct LocaleAuditFixture {
    pub report: Vec<u8>,
    pub plan: Vec<u8>,
    pub evidence: Option<Vec<u8>>,
    pub assessment: Vec<u8>,
}

/// Builds one exact report-bound locale coverage audit for controller tests.
#[must_use]
pub fn locale_audit(with_evidence: bool) -> Option<LocaleAuditFixture> {
    let binding = report_binding()?;
    let mut plan = LocaleCoveragePlan::parse(PLAN).ok()?;
    plan.payload.report_payload_digest = binding.payload_digest;
    plan.payload.docs = binding.docs;
    let plan_bytes = plan.payload.emit().ok()?;
    let plan = LocaleCoveragePlan::parse(&plan_bytes).ok()?;
    let evidence = if with_evidence {
        Some(coverage_evidence(&plan)?)
    } else {
        None
    };
    let assessment = assess(
        &plan,
        evidence.as_ref(),
        env!("CARGO_PKG_VERSION"),
        Digest::from([32; 32]),
    )
    .ok()?;
    Some(LocaleAuditFixture {
        report: binding.report,
        plan: plan_bytes,
        evidence: evidence
            .as_ref()
            .map(|envelope| envelope.payload.emit())
            .transpose()
            .ok()?,
        assessment,
    })
}

fn coverage_evidence(
    plan: &Envelope<LocaleCoveragePlan>,
) -> Option<Envelope<LocaleCoverageEvidence>> {
    let mut evidence = LocaleCoverageEvidence::parse(EVIDENCE).ok()?;
    evidence.payload.plan_payload_digest = plan.payload_digest;
    evidence.payload.docs = plan.payload.docs.clone();
    let value = evidence.payload.emit().ok()?;
    LocaleCoverageEvidence::parse(&value).ok()
}
