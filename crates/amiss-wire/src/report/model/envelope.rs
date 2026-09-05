use serde::{Deserialize, Serialize};

use crate::digest::Digest;

use super::{
    AnalysisError, Controls, DocumentGitMode, DocumentResult, DocumentSide, Engine, Evaluation,
    Feedback, Finding, FindingFactEvidence, ObservationComparison, Occurrence,
    ProjectionDifference, ProjectionSource, RepoPath, Resolution, Summary,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportEnvelopeSchema {
    #[serde(rename = "amiss/scanner-report-envelope")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportPayloadSchema {
    #[serde(rename = "amiss/scanner-report-payload")]
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportCompatibility {
    #[serde(rename = "1")]
    One,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub observations: Vec<ObservationComparison<Occurrence<P, R>>>,
    pub result: ReportResult,
    pub schema: ReportPayloadSchema,
    pub summary: Summary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "lowercase")]
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
