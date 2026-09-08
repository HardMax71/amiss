use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Label {
    pub id: u64,
    pub name: String,
    pub exclusive: bool,
    pub is_archived: bool,
    pub color: String,
    pub description: String,
    pub url: String,
}

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(deserialize_with = "Option::deserialize")]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Milestone {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub state: IssueState,
    pub open_issues: u64,
    pub closed_issues: u64,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub due_on: Option<String>,
}
