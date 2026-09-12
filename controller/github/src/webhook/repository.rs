use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::{Absent, deserialize_some};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "Kind: Deserialize<'de>"))]
pub struct WorkflowOwner<Kind = AccountKind> {
    pub login: String,
    pub id: UInt,
    #[serde(default, skip_serializing)]
    pub slug: Absent,
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
    pub kind: Option<Kind>,
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
