use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerRecord {
    pub login: String,
    pub id: UInt,
    pub node_id: String,
    pub avatar_url: String,
    pub gravatar_id: Nullable<String>,
    pub url: String,
    pub html_url: String,
    pub followers_url: String,
    pub following_url: String,
    pub gists_url: String,
    pub starred_url: String,
    pub subscriptions_url: String,
    pub organizations_url: String,
    pub repos_url: String,
    pub events_url: String,
    pub received_events_url: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub site_admin: bool,
    pub name: Option<Nullable<String>>,
    pub email: Option<Nullable<String>>,
    pub starred_at: Option<String>,
    pub user_view_type: Option<String>,
}
