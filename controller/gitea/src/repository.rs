use amiss_wire::assessment::Nullable;
use amiss_wire::model::ObjectFormat;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::user::UserRecord;

mod settings;
mod transfer;

pub use settings::{
    ExternalTracker, ExternalWiki, InternalTracker, MergeStyle, ProjectsMode,
    RepositoryPermissions, TrackerStyle, UpdateStyle,
};
pub use transfer::{Organization, PermissionLevel, RepositoryTransfer, RepositoryUnit, Team};

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the repository API exposes independent feature flags"
)]
pub struct RepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: UserRecord,
    pub description: String,
    pub empty: bool,
    pub private: bool,
    pub fork: bool,
    pub template: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent: Option<Nullable<Box<RepositoryRecord>>>,
    pub mirror: bool,
    pub size: u64,
    pub language: String,
    pub languages_url: String,
    pub html_url: String,
    pub url: String,
    pub link: String,
    pub ssh_url: String,
    pub clone_url: String,
    pub original_url: String,
    pub website: String,
    pub stars_count: u64,
    pub forks_count: u64,
    pub watchers_count: u64,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub branch_count: Option<UInt>,
    pub open_issues_count: u64,
    pub open_pr_counter: u64,
    pub release_counter: u64,
    pub default_branch: String,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_target_branch: Option<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: String,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub permissions: Option<RepositoryPermissions>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub has_code: Option<bool>,
    pub has_issues: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub internal_tracker: Option<InternalTracker>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_tracker: Option<ExternalTracker>,
    pub has_wiki: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub external_wiki: Option<ExternalWiki>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub has_wiki_contents: Option<bool>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub wiki_branch: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub wiki_ssh_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub wiki_clone_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub globally_editable_wiki: Option<bool>,
    pub has_pull_requests: bool,
    pub has_projects: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub projects_mode: Option<ProjectsMode>,
    pub has_releases: bool,
    pub has_packages: bool,
    pub has_actions: bool,
    pub ignore_whitespace_conflicts: bool,
    pub allow_merge_commits: bool,
    pub allow_rebase: bool,
    pub allow_rebase_explicit: bool,
    pub allow_squash_merge: bool,
    pub allow_fast_forward_only_merge: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_merge_update: Option<bool>,
    pub allow_rebase_update: bool,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_manual_merge: Option<bool>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub autodetect_manual_merge: Option<bool>,
    pub default_delete_branch_after_merge: bool,
    pub default_merge_style: MergeStyle,
    pub default_update_style: UpdateStyle,
    pub default_allow_maintainer_edit: bool,
    pub avatar_url: String,
    pub internal: bool,
    pub mirror_interval: String,
    pub object_format_name: ObjectFormat,
    pub mirror_updated: String,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub mirror_last_sync_at: Option<String>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub repo_transfer: Option<Nullable<RepositoryTransfer>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub topics: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub licenses: Option<Nullable<Vec<String>>>,
}
