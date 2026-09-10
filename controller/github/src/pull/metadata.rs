use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::owner::OwnerRecord;

use super::State;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabelRecord {
    pub id: UInt,
    pub node_id: String,
    pub url: String,
    pub name: String,
    pub description: Nullable<String>,
    pub color: String,
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MilestoneRecord<User = OwnerRecord> {
    pub url: String,
    pub html_url: String,
    pub labels_url: String,
    pub id: UInt,
    pub node_id: String,
    pub number: UInt,
    pub state: State,
    pub title: String,
    pub description: Nullable<String>,
    pub creator: Nullable<User>,
    pub open_issues: UInt,
    pub closed_issues: UInt,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Nullable<String>,
    pub due_on: Nullable<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    bound(
        deserialize = "User: Deserialize<'de>, Message: Deserialize<'de>, Title: Deserialize<'de>"
    )
)]
pub struct AutoMergeRecord<User = OwnerRecord, Message = String, Title = Message> {
    #[serde(deserialize_with = "User::deserialize")]
    pub enabled_by: User,
    pub merge_method: MergeMethod,
    #[serde(deserialize_with = "Title::deserialize")]
    pub commit_title: Title,
    #[serde(deserialize_with = "Message::deserialize")]
    pub commit_message: Message,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestLinks {
    pub comments: Link,
    pub commits: Link,
    pub statuses: Link,
    pub html: Link,
    pub issue: Link,
    pub review_comments: Link,
    pub review_comment: Link,
    #[serde(rename = "self")]
    pub self_link: Link,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub href: String,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TeamRecord {
    pub id: UInt,
    pub node_id: String,
    pub url: String,
    pub members_url: String,
    pub name: String,
    pub description: Nullable<String>,
    pub permission: String,
    pub html_url: String,
    pub repositories_url: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: TeamKind,
    pub privacy: Option<String>,
    pub notification_setting: Option<String>,
    pub ldap_dn: Option<String>,
    pub organization_id: Option<UInt>,
    pub enterprise_id: Option<UInt>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StackRecord {
    pub base: StackBase,
    pub size: Option<UInt>,
    pub position: Option<UInt>,
    pub id: Option<UInt>,
    pub number: Option<UInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StackBase {
    #[serde(rename = "ref")]
    pub branch: String,
    pub sha: Oid,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum TeamKind {
    Enterprise,
    Organization,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorAssociation {
    Collaborator,
    Contributor,
    FirstTimer,
    FirstTimeContributor,
    Mannequin,
    Member,
    None,
    Owner,
}
