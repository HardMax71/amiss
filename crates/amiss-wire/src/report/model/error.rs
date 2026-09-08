use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
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
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct AnalysisError<P = RepoPath> {
    pub code: AnalysisErrorCode,
    #[serde(with = "As::<Option<TryFromInto<UInt>>>")]
    pub configured_limit: Option<u64>,
    pub description: String,
    #[serde(with = "As::<Option<TryFromInto<UInt>>>")]
    pub observed_lower_bound: Option<u64>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<P>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub path_bytes_hex: Option<String>,
    pub phase: AnalysisPhase,
    #[serde(deserialize_with = "Option::deserialize")]
    pub resource: Option<ResourceName>,
}
