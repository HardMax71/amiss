mod assessment;
mod evidence;
mod plan;

pub use crate::report::ReportDefect as PlanDefect;
pub use assessment::{
    AssessDefect, AssessmentDefect, ExternalAssessment, ExternalAssessmentEnvelope,
    ExternalAssessmentEnvelopeSchema, ExternalAssessmentPayloadSchema, ExternalAssessmentSubject,
    ExternalReason, ExternalVerdict, ExternalVerdictRow, assess, parse_assessment,
};
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
