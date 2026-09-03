use crate::json::Value;

mod assessment;
mod evidence;
mod plan;

pub use crate::report::ReportDefect as PlanDefect;
pub use assessment::{AssessDefect, assess};
pub use evidence::{
    EvidenceDefect, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ExternalEvidenceSchema, ForgeRepository, ForgeTail, ProbeFailure, ProbeMethod, evidence,
    parse_evidence,
};
pub use plan::{
    ExternalDestination, ExternalEngine, ExternalPlan, ExternalPlanEnvelope,
    ExternalPlanEnvelopeSchema, ExternalPlanPayloadSchema, ExternalPlanReport, ExternalRepository,
    parse_plan, plan,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/external-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/external-plan-payload";
pub const EVIDENCE_SCHEMA: &str = "amiss/external-evidence";
pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/external-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/external-assessment-payload";
pub const EXTERNAL_DOCUMENT_BYTES: u64 = crate::report::MACHINE_JSON_BYTES;

/// An external object in canonical member order; the parser demands sorted keys,
/// so the builder sorts rather than trusting the spelling site.
fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::Object(members.into_boxed_slice())
}

fn string(value: &str) -> Value {
    Value::String(value.into())
}
