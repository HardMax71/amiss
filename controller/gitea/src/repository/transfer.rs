use std::collections::BTreeMap;

use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::user::{UserRecord, UserVisibility};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryTransfer {
    #[serde(deserialize_with = "Option::deserialize")]
    pub doer: Option<UserRecord>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub recipient: Option<UserRecord>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub teams: Option<Vec<Team>>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: u64,
    pub name: String,
    pub description: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub organization: Option<Organization>,
    pub includes_all_repositories: bool,
    pub permission: PermissionLevel,
    #[serde(deserialize_with = "Option::deserialize")]
    pub units: Option<Vec<RepositoryUnit>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub units_map: Option<BTreeMap<RepositoryUnit, PermissionLevel>>,
    pub can_create_org_repo: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub visibility: Option<UserVisibility>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub email: String,
    pub avatar_url: String,
    pub description: String,
    pub website: String,
    pub location: String,
    pub visibility: UserVisibility,
    pub repo_admin_change_team_access: bool,
    pub username: String,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub created: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    None,
    Read,
    Write,
    Admin,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum RepositoryUnit {
    #[serde(rename = "repo.actions")]
    Actions,
    #[serde(rename = "repo.packages")]
    Packages,
    #[serde(rename = "repo.code")]
    Code,
    #[serde(rename = "repo.issues")]
    Issues,
    #[serde(rename = "repo.ext_issues")]
    ExternalIssues,
    #[serde(rename = "repo.wiki")]
    Wiki,
    #[serde(rename = "repo.pulls")]
    Pulls,
    #[serde(rename = "repo.releases")]
    Releases,
    #[serde(rename = "repo.projects")]
    Projects,
    #[serde(rename = "repo.ext_wiki")]
    ExternalWiki,
}
