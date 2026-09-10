use std::collections::BTreeMap;

use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{
    As, DeserializeFromStr, MapPreventDuplicates, Same, SerializeDisplay, TryFromInto,
};
use strum::{Display, EnumString};

use crate::owner::OwnerRecord;
use crate::workflow::WorkflowPullRequest;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    deserialize = "Resource: Deserialize<'de>, Suite: Deserialize<'de>, App: Deserialize<'de>, Conclusion: Deserialize<'de>"
))]
pub struct CheckRunRecord<
    Resource = CheckRunResource,
    Suite = CheckRunSuite,
    App = CheckRunApp,
    Conclusion = CheckRunConclusion,
> {
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
    #[serde(flatten)]
    pub resource: Resource,
    pub url: String,
    pub html_url: Nullable<String>,
    pub started_at: Nullable<String>,
    pub completed_at: Nullable<String>,
    pub check_suite: Nullable<Suite>,
    pub pull_requests: Vec<WorkflowPullRequest>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub deployment: Option<CheckRunDeployment<App>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunResource {
    pub node_id: String,
    pub details_url: Nullable<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunPage {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub total_count: u64,
    pub check_runs: Vec<CheckRunRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunOutputRecord {
    #[serde(deserialize_with = "Option::deserialize")]
    pub title: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub summary: Option<String>,
    pub text: Nullable<String>,
    pub annotations_count: UInt,
    pub annotations_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunSuite {
    pub id: UInt,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "App: Deserialize<'de>"))]
pub struct CheckRunDeployment<App = CheckRunApp> {
    pub id: UInt,
    pub node_id: String,
    pub task: String,
    pub environment: String,
    pub description: Nullable<String>,
    pub statuses_url: String,
    pub repository_url: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub original_environment: Option<String>,
    pub transient_environment: Option<bool>,
    pub production_environment: Option<bool>,
    pub performed_via_github_app: Option<Nullable<App>>,
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

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunApp {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
    pub owner: AppOwner,
    pub name: String,
    pub description: Nullable<String>,
    pub external_url: String,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(with = "As::<MapPreventDuplicates<Same, Same>>")]
    pub permissions: BTreeMap<String, String>,
    pub events: Vec<String>,
    pub slug: Option<String>,
    pub client_id: Option<String>,
    pub installations_count: Option<UInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AppOwner {
    Account(Box<OwnerRecord>),
    Enterprise(Box<EnterpriseRecord>),
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
