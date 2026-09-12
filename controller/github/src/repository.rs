use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::owner::OwnerRecord;

pub mod metadata;
pub mod pull;
pub mod template;

use metadata::{
    CodeOfConduct, CustomProperties, PullRequestCreationPolicy, RepositoryLicense,
    RepositoryPermissions, SecurityAnalysis,
};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RepositoryRecord {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: OwnerRecord,
    pub default_branch: String,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRepositoryRecord {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub full_name: String,
    pub owner: OwnerRecord,
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
    pub allow_forking: Option<bool>,
    pub archived: Option<bool>,
    pub clone_url: Option<String>,
    pub code_of_conduct: Option<CodeOfConduct>,
    pub created_at: Option<Nullable<String>>,
    pub custom_properties: Option<CustomProperties>,
    pub default_branch: Option<String>,
    pub delete_branch_on_merge: Option<bool>,
    pub disabled: Option<bool>,
    pub forks: Option<UInt>,
    pub forks_count: Option<UInt>,
    pub git_url: Option<String>,
    pub has_discussions: Option<bool>,
    pub has_downloads: Option<bool>,
    pub has_issues: Option<bool>,
    pub has_pages: Option<bool>,
    pub has_projects: Option<bool>,
    pub has_pull_requests: Option<bool>,
    pub has_wiki: Option<bool>,
    pub homepage: Option<Nullable<String>>,
    pub is_template: Option<bool>,
    pub language: Option<Nullable<String>>,
    pub license: Option<Nullable<RepositoryLicense>>,
    pub mirror_url: Option<Nullable<String>>,
    pub network_count: Option<UInt>,
    pub open_issues: Option<UInt>,
    pub open_issues_count: Option<UInt>,
    pub permissions: Option<RepositoryPermissions>,
    pub pull_request_creation_policy: Option<PullRequestCreationPolicy>,
    pub pushed_at: Option<Nullable<String>>,
    pub role_name: Option<String>,
    pub security_and_analysis: Option<Nullable<SecurityAnalysis>>,
    pub size: Option<UInt>,
    pub ssh_url: Option<String>,
    pub stargazers_count: Option<UInt>,
    pub subscribers_count: Option<UInt>,
    pub svn_url: Option<String>,
    pub temp_clone_token: Option<String>,
    pub topics: Option<Vec<String>>,
    pub updated_at: Option<Nullable<String>>,
    pub visibility: Option<String>,
    pub watchers: Option<UInt>,
    pub watchers_count: Option<UInt>,
    pub web_commit_signoff_required: Option<bool>,
}
