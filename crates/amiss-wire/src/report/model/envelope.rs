use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto, apply};

use crate::controls::ProjectionSource;
use crate::envelope::MACHINE_JSON_BYTES;
use crate::envelope::{Envelope, Payload, Sealing};
use crate::report::{PAYLOAD_SCHEMA, ReportDefect, result_verdict};

use super::{
    AnalysisError, Controls, DocumentGitMode, DocumentResult, DocumentSide, Engine, Evaluation,
    Feedback, Finding, FindingFactEvidence, ObservationComparison, ProjectionDifference,
    ReportResolution, Summary,
};

use crate::model::RepoPath;

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
    #[strum(serialize = "3")]
    Three,
}

pub type ReportEnvelope<P = ReportPayload> = Envelope<P>;

impl<P, R, M, E> Payload for ReportPayload<P, R, M, E>
where
    Self: Serialize,
    P: PartialEq,
    R: PartialEq,
{
    type Schema = ReportEnvelopeSchema;
    type Defect = ReportDefect;
    const DOMAIN: &'static str = PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = MACHINE_JSON_BYTES;
    const SEALING: Sealing = Sealing::Exact;

    fn validate(&self) -> Result<(), ReportDefect> {
        result_verdict(&self.result)?;
        super::comparisons_valid(&self.observations)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportPayload<
    P = RepoPath,
    R = ReportResolution<P>,
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

#[apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportResult {
    pub complete: bool,
    pub error_count: u64,
    pub exit_code: u8,
    pub finding_count: u64,
    pub status: ReportStatus,
}
