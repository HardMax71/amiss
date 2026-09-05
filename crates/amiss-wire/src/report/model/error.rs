use serde::{Deserialize, Serialize};

use crate::controls::ResourceName;

use super::super::AnalysisErrorCode;
use super::RepoPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "lowercase")]
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
pub struct AnalysisError {
    pub code: AnalysisErrorCode,
    #[serde(deserialize_with = "Option::deserialize")]
    pub configured_limit: Option<u64>,
    pub description: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub observed_lower_bound: Option<u64>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<RepoPath>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub path_bytes_hex: Option<String>,
    pub phase: AnalysisPhase,
    #[serde(deserialize_with = "Option::deserialize")]
    pub resource: Option<ResourceName>,
}
