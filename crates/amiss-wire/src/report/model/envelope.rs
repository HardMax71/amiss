use serde::{Deserialize, Serialize};

use crate::digest::Digest;

use super::{
    AnalysisError, Controls, DocumentGitMode, DocumentResult, DocumentSide, Engine, Evaluation,
    Feedback, Finding, FindingFactEvidence, ObservationComparison, ProjectionDifference,
    ProjectionSource, RepoPath, Resolution, Summary,
};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
)]
pub enum ReportEnvelopeSchema {
    #[strum(serialize = "amiss/scanner-report-envelope")]
    Current,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
)]
pub enum ReportPayloadSchema {
    #[strum(serialize = "amiss/scanner-report-payload")]
    Current,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
)]
pub enum ReportCompatibility {
    #[strum(serialize = "1")]
    One,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportEnvelope<P = ReportPayload> {
    pub payload: P,
    pub payload_digest: Digest,
    pub schema: ReportEnvelopeSchema,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPayload<
    P = RepoPath,
    R = Resolution<P>,
    M = DocumentGitMode,
    E = FindingFactEvidence<P, R, ProjectionSource, ProjectionDifference, M>,
> {
    pub compatibility: ReportCompatibility,
    pub controls: Controls,
    pub documents: Vec<DocumentResult<P, DocumentSide<M>>>,
    pub engine: Engine,
    pub errors: Vec<AnalysisError<P>>,
    pub evaluation: Evaluation,
    pub feedback: Feedback<P>,
    pub findings: Vec<Finding<P, E>>,
    pub observations: Vec<ObservationComparison<P, R>>,
    pub result: ReportResult,
    pub schema: ReportPayloadSchema,
    pub summary: Summary,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
    strum::EnumString,
    strum::AsRefStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum ReportStatus {
    Fail,
    Incomplete,
    Pass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportResult {
    pub complete: bool,
    pub error_count: u64,
    pub exit_code: u8,
    pub finding_count: u64,
    pub status: ReportStatus,
}
