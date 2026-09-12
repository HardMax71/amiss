use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct IssuePullRequest {
    pub diff_url: Option<String>,
    pub html_url: Option<String>,
    pub merged_at: Option<Nullable<String>>,
    pub patch_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SubIssuesSummary {
    pub total: UInt,
    pub completed: UInt,
    pub percent_completed: UInt,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct IssueDependenciesSummary {
    pub blocked_by: UInt,
    pub blocking: UInt,
    pub total_blocked_by: UInt,
    pub total_blocking: UInt,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct IssueType {
    pub id: UInt,
    pub node_id: String,
    pub name: String,
    pub description: Nullable<String>,
    pub color: Option<Nullable<IssueTypeColor>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum IssueTypeColor {
    Gray,
    Blue,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
}
