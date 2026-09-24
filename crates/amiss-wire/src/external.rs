mod assessment;
mod evidence;
mod plan;

pub use assessment::{
    AssessDefect, AssessmentDefect, ExternalAssessment, ExternalAssessmentEnvelopeSchema,
    ExternalAssessmentPayloadSchema, ExternalAssessmentSubject, ExternalReason, ExternalVerdict,
    ExternalVerdictRow, assess,
};
pub use evidence::{
    EvidenceDefect, ExternalEvidence, ExternalEvidenceProducer, ExternalEvidenceRow,
    ExternalEvidenceSchema, ForgeRepository, ForgeTail, ProbeFailure, ProbeMethod, evidence,
};
pub use plan::{
    ExternalDestination, ExternalEngine, ExternalPlan, ExternalPlanEnvelopeSchema,
    ExternalPlanPayloadSchema, ExternalPlanReport, ExternalRepository, plan,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/external-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/external-plan-payload";
pub const EVIDENCE_SCHEMA: &str = "amiss/external-evidence";
pub const ASSESSMENT_ENVELOPE_SCHEMA: &str = "amiss/external-assessment-envelope";
pub const ASSESSMENT_PAYLOAD_SCHEMA: &str = "amiss/external-assessment-payload";
pub const EXTERNAL_DOCUMENT_BYTES: u64 = crate::envelope::MACHINE_JSON_BYTES;
