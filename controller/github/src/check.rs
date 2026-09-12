use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "App: Deserialize<'de>, Conclusion: Deserialize<'de>"))]
pub struct CheckRunRecord<App = CheckRunApp, Conclusion = CheckRunConclusion> {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub name: String,
    pub head_sha: Oid,
    #[serde(deserialize_with = "Option::deserialize")]
    pub external_id: Option<String>,
    pub status: CheckRunStatus,
    #[serde(deserialize_with = "Option::deserialize")]
    pub conclusion: Option<Conclusion>,
    pub output: CheckRunOutputRecord,
    #[serde(deserialize_with = "Option::deserialize")]
    pub app: Option<App>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CheckRunPage {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub total_count: u64,
    pub check_runs: Vec<CheckRunRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CheckRunOutputRecord {
    #[serde(deserialize_with = "Option::deserialize")]
    pub title: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub summary: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum CheckRunConclusion {
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Success,
    Skipped,
    Stale,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CheckRunApp {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnterpriseRecord {
    pub id: UInt,
    pub node_id: String,
    pub name: String,
    pub slug: String,
    pub html_url: String,
    pub created_at: Nullable<String>,
    pub updated_at: Nullable<String>,
    pub avatar_url: String,
    pub description: Option<Nullable<String>>,
    pub website_url: Option<Nullable<String>>,
}
