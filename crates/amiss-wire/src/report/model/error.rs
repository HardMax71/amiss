use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::controls::ResourceName;

use super::super::AnalysisErrorCode;
use super::RepoPath;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum AnalysisPhase {
    Configuration,
    Discovery,
    Git,
    Internal,
    Invocation,
    Output,
    Parse,
    Policy,
    Resolution,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisError<P = RepoPath> {
    pub code: AnalysisErrorCode,
    pub configured_limit: Option<u64>,
    pub description: String,
    pub observed_lower_bound: Option<u64>,
    pub path: Option<P>,
    pub path_bytes_hex: Option<String>,
    pub phase: AnalysisPhase,
    pub resource: Option<ResourceName>,
}
