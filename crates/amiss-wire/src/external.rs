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

fn read<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, crate::de::Error> {
    use crate::de::{Error, ErrorKind, fail};
    use crate::json;

    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > EXTERNAL_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    if !matches!(
        json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?,
        json::Value::Object(_)
    ) {
        return fail("$", ErrorKind::WrongType);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    // The strict gate has already enforced the document depth ceiling.
    deserializer.disable_recursion_limit();
    serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| crate::de::deserialize_error("$", &defect))
}
