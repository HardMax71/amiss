use amiss_wire::report::model::{Engine, ReportPayloadSchema, ReportResult, UnavailableStatus};
use amiss_wire::requests::GitSnapshotIdentity;
use serde::{Deserialize, de::IgnoredAny};

#[derive(Deserialize)]
#[serde(transparent, bound(deserialize = "T: Deserialize<'de>"))]
pub(super) struct Object<T> {
    #[serde(deserialize_with = "object::deserialize")]
    pub(super) fields: T,
}

serde_with::with_prefix!(object "");

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
pub(super) struct ResultPayload {
    pub(super) result: Object<ReportResult>,
}

#[derive(Deserialize)]
pub(super) struct Findings {
    pub(super) findings: Vec<IgnoredAny>,
}
