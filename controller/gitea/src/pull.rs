use amiss_wire::model::Oid;
use js_int::{Int, UInt};
use serde::{Deserialize, Serialize};
use serde_with::{As, NoneAsEmptyString, TryFromInto};

use crate::issue::{IssueState, Label, Milestone};
use crate::repository::{RepositoryRecord, Team};
use crate::user::UserRecord;

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<Vec<_>> => #[serde(deserialize_with = "Option::deserialize")],
    Option<UserRecord> => #[serde(deserialize_with = "Option::deserialize")],
    Option<Milestone> => #[serde(deserialize_with = "Option::deserialize")],
    Option<String> => #[serde(deserialize_with = "Option::deserialize")],
    Option<Oid> => #[serde(deserialize_with = "Option::deserialize")],
    Option<UInt> => #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )],
    Option<Int> => #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the pull API exposes independent state and permission flags"
)]
pub struct PullRequestRecord {
    pub id: u64,
    pub url: String,
    pub number: u64,
    pub user: Option<UserRecord>,
    pub title: String,
    pub body: String,
    pub labels: Option<Vec<Label>>,
    pub milestone: Option<Milestone>,
    pub assignee: Option<UserRecord>,
    pub assignees: Option<Vec<UserRecord>>,
    pub requested_reviewers: Option<Vec<UserRecord>>,
    pub requested_reviewers_teams: Option<Vec<Team>>,
    pub state: IssueState,
    pub draft: bool,
    pub is_locked: bool,
    pub comments: u64,
    pub review_comments: Option<UInt>,
    pub additions: Option<UInt>,
    pub deletions: Option<UInt>,
    pub changed_files: Option<UInt>,
    pub html_url: String,
    pub diff_url: String,
    pub patch_url: String,
    pub mergeable: bool,
    pub merged: bool,
    pub merged_at: Option<String>,
    pub merge_commit_sha: Option<Oid>,
    pub merged_by: Option<UserRecord>,
    pub allow_maintainer_edit: bool,
    pub base: PullRefRecord,
    pub head: PullRefRecord,
    #[serde_with(skip_apply)]
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub merge_base: Option<Oid>,
    pub due_date: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub closed_at: Option<String>,
    pub pin_order: Int,
    pub content_version: Option<UInt>,
    pub flow: Option<Int>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRefRecord {
    pub label: String,
    #[serde(rename = "ref")]
    pub branch: String,
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub sha: Option<Oid>,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub repo_id: i64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub repo: Option<RepositoryRecord>,
}
