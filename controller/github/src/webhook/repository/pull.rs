use amiss_wire::assessment::Nullable;
use js_int::{Int, UInt};
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use super::WorkflowOwner;
use crate::repository::metadata::{
    CustomProperties, MergeCommitMessage, MergeCommitTitle, PullRequestCreationPolicy,
    RepositoryAccess, SquashMergeCommitMessage, SquashMergeCommitTitle,
};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the webhook repository declares independent feature flags"
)]
pub struct PullRepository {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub name: String,
    pub full_name: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub owner: Option<WorkflowOwner>,
    pub node_id: String,
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
    pub clone_url: String,
    pub default_branch: String,
    pub forks: UInt,
    pub forks_count: UInt,
    pub git_url: String,
    pub has_downloads: bool,
    pub has_issues: bool,
    pub has_projects: bool,
    pub has_wiki: bool,
    pub has_pages: bool,
    pub homepage: Nullable<String>,
    pub language: Nullable<String>,
    pub archived: bool,
    pub mirror_url: Nullable<String>,
    pub open_issues: UInt,
    pub open_issues_count: UInt,
    pub license: Nullable<License>,
    pub pushed_at: Nullable<Timestamp>,
    pub size: UInt,
    pub ssh_url: String,
    pub stargazers_count: UInt,
    pub svn_url: String,
    pub watchers: UInt,
    pub watchers_count: UInt,
    pub created_at: Timestamp,
    pub updated_at: String,
    pub topics: Vec<String>,
    pub visibility: Visibility,
    pub allow_auto_merge: Option<bool>,
    pub allow_forking: Option<bool>,
    pub allow_merge_commit: Option<bool>,
    pub allow_rebase_merge: Option<bool>,
    pub allow_squash_merge: Option<bool>,
    pub allow_update_branch: Option<bool>,
    pub custom_properties: Option<CustomProperties>,
    pub delete_branch_on_merge: Option<bool>,
    pub disabled: Option<bool>,
    pub has_discussions: Option<bool>,
    pub has_pull_requests: Option<bool>,
    pub is_template: Option<bool>,
    pub master_branch: Option<String>,
    pub merge_commit_message: Option<MergeCommitMessage>,
    pub merge_commit_title: Option<MergeCommitTitle>,
    pub organization: Option<String>,
    pub permissions: Option<RepositoryAccess>,
    pub public: Option<bool>,
    pub pull_request_creation_policy: Option<PullRequestCreationPolicy>,
    pub role_name: Option<Nullable<String>>,
    pub squash_merge_commit_message: Option<SquashMergeCommitMessage>,
    pub squash_merge_commit_title: Option<SquashMergeCommitTitle>,
    pub stargazers: Option<UInt>,
    pub use_squash_pr_title_as_default: Option<bool>,
    pub web_commit_signoff_required: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Timestamp {
    Seconds(Int),
    DateTime(String),
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct License {
    pub key: String,
    pub name: String,
    pub node_id: String,
    pub spdx_id: String,
    pub url: Nullable<String>,
}
