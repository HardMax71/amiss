use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::{Absent, deserialize_some};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::repository::WorkflowOwner;

pub mod request;
pub mod review;
pub mod thread;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum LockReason {
    Resolved,
    OffTopic,
    #[strum(serialize = "too heated")]
    TooHeated,
    Spam,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PullRequestAccountKind {
    Bot,
    User,
    Organization,
    Mannequin,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Reviewer<Account = WorkflowOwner<PullRequestAccountKind>, Team = ReviewTeam> {
    Account(Box<Account>),
    Team(Box<Team>),
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Team {
    pub name: String,
    pub id: UInt,
    #[serde(default, skip_serializing)]
    pub login: Absent,
    pub node_id: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Nullable<String>>,
    pub privacy: Option<TeamPrivacy>,
    pub url: Option<String>,
    pub html_url: Option<String>,
    pub members_url: Option<String>,
    pub repositories_url: Option<String>,
    pub permission: Option<String>,
    pub parent: Option<Nullable<Box<ParentTeam>>>,
    pub deleted: Option<bool>,
    pub notification_setting: Option<TeamNotifications>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReviewTeam {
    pub name: String,
    pub id: UInt,
    #[serde(default, skip_serializing)]
    pub login: Absent,
    pub node_id: String,
    pub slug: String,
    pub description: Nullable<String>,
    pub privacy: TeamPrivacy,
    pub url: String,
    pub html_url: String,
    pub members_url: String,
    pub repositories_url: String,
    pub permission: String,
    pub parent: Option<Nullable<Box<ParentTeam>>>,
    pub deleted: Option<bool>,
    pub notification_setting: Option<TeamNotifications>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ParentTeam {
    pub name: String,
    pub id: UInt,
    pub node_id: String,
    pub slug: String,
    pub description: Nullable<String>,
    pub privacy: TeamPrivacy,
    pub url: String,
    pub html_url: String,
    pub members_url: String,
    pub repositories_url: String,
    pub permission: String,
    pub notification_setting: Option<TeamNotifications>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum TeamPrivacy {
    Open,
    Closed,
    Secret,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum TeamNotifications {
    NotificationsEnabled,
    NotificationsDisabled,
}
