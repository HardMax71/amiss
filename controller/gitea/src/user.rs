use js_int::{Int, UInt};
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    i64 => #[serde(with = "As::<TryFromInto<Int>>")],
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the provider exposes independent account flags"
)]
pub struct UserRecord {
    pub id: u64,
    pub login: String,
    pub login_name: String,
    pub source_id: i64,
    pub full_name: String,
    pub email: String,
    pub avatar_url: String,
    pub html_url: String,
    pub language: String,
    pub is_admin: bool,
    pub last_login: String,
    pub created: String,
    pub restricted: bool,
    pub active: bool,
    pub prohibit_login: bool,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    pub website: String,
    pub description: String,
    pub visibility: UserVisibility,
    pub followers_count: u64,
    pub following_count: u64,
    pub starred_repos_count: u64,
    pub username: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserVisibility {
    Public,
    Limited,
    Private,
}
