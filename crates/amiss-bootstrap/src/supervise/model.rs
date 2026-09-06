use amiss_wire::report::model::{Engine, ReportPayloadSchema, UnavailableStatus};
use amiss_wire::requests::GitSnapshotIdentity;
use serde::{Deserialize, de::IgnoredAny};

#[derive(Deserialize)]
pub(super) struct Object<T> {
    // Open projections only: flatten masks an inner deny_unknown_fields guard.
    #[serde(flatten)]
    pub(super) fields: T,
}

#[derive(Deserialize)]
pub(super) struct PayloadHeader {
    #[serde(rename = "schema")]
    _schema: ReportPayloadSchema,
}

#[derive(Deserialize)]
pub(super) struct EnginePayload {
    pub(super) engine: Object<Engine>,
}

#[derive(Deserialize)]
pub(super) struct EvaluationStatus {
    pub(super) status: Option<UnavailableStatus>,
}

#[derive(Deserialize)]
pub(super) struct BaseEvaluation {
    pub(super) base: Object<GitSnapshotIdentity>,
}

#[derive(Deserialize)]
pub(super) struct CandidateEvaluation<C = Object<GitSnapshotIdentity>> {
    pub(super) candidate: C,
}

#[derive(Deserialize)]
pub(super) struct ResultPayload<R> {
    pub(super) result: Object<R>,
}

#[derive(Deserialize)]
pub(super) struct Completion {
    pub(super) complete: bool,
    pub(super) exit_code: i64,
}

#[derive(Deserialize)]
pub(super) struct FindingCount {
    pub(super) finding_count: i64,
}

#[derive(Deserialize)]
pub(super) struct Findings {
    pub(super) findings: Vec<IgnoredAny>,
}
