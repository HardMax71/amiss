use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRepository {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub full_name: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub owner: Option<WorkflowOwner>,
    pub private: bool,
    pub html_url: String,
    pub description: Nullable<String>,
    pub fork: bool,
    pub url: String,
    pub archive_url: String,
    pub assignees_url: String,
    pub blobs_url: String,
    pub branches_url: String,
    pub collaborators_url: String,
    pub comments_url: String,
    pub commits_url: String,
    pub compare_url: String,
    pub contents_url: String,
    pub contributors_url: String,
    pub deployments_url: String,
    pub downloads_url: String,
    pub events_url: String,
    pub forks_url: String,
    pub git_commits_url: String,
    pub git_refs_url: String,
    pub git_tags_url: String,
    pub hooks_url: String,
    pub issue_comment_url: String,
    pub issue_events_url: String,
    pub issues_url: String,
    pub keys_url: String,
    pub labels_url: String,
    pub languages_url: String,
    pub merges_url: String,
    pub milestones_url: String,
    pub notifications_url: String,
    pub pulls_url: String,
    pub releases_url: String,
    pub stargazers_url: String,
    pub statuses_url: String,
    pub subscribers_url: String,
    pub subscription_url: String,
    pub tags_url: String,
    pub teams_url: String,
    pub trees_url: String,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowOwner {
    pub login: String,
    pub id: UInt,
    pub avatar_url: Option<String>,
    pub deleted: Option<bool>,
    pub email: Option<Nullable<String>>,
    pub events_url: Option<String>,
    pub followers_url: Option<String>,
    pub following_url: Option<String>,
    pub gists_url: Option<String>,
    pub gravatar_id: Option<String>,
    pub html_url: Option<String>,
    pub name: Option<String>,
    pub node_id: Option<String>,
    pub organizations_url: Option<String>,
    pub received_events_url: Option<String>,
    pub repos_url: Option<String>,
    pub site_admin: Option<bool>,
    pub starred_url: Option<String>,
    pub subscriptions_url: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<AccountKind>,
    pub url: Option<String>,
    pub user_view_type: Option<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AccountKind {
    Bot,
    User,
    Organization,
}
