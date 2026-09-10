use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

pub mod metadata;

use crate::owner::OwnerRecord;
use crate::repository::pull::PullRepositoryRecord;
use metadata::{
    AuthorAssociation, AutoMergeRecord, LabelRecord, MilestoneRecord, PullRequestLinks,
    StackRecord, TeamRecord,
};

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")],
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestRecord {
    pub id: u64,
    pub number: u64,
    pub state: State,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub mergeable: Option<bool>,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub merge_commit_sha: Option<Oid>,
    pub head: PullRefRecord,
    pub base: PullRefRecord,
    pub url: String,
    pub node_id: String,
    pub html_url: String,
    pub diff_url: String,
    pub patch_url: String,
    pub issue_url: String,
    pub commits_url: String,
    pub review_comments_url: String,
    pub review_comment_url: String,
    pub comments_url: String,
    pub statuses_url: String,
    pub locked: bool,
    pub title: String,
    pub user: OwnerRecord,
    pub body: Nullable<String>,
    pub labels: Vec<LabelRecord>,
    pub milestone: Nullable<Box<MilestoneRecord>>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Nullable<String>,
    pub merged_at: Nullable<String>,
    pub assignee: Nullable<Box<OwnerRecord>>,
    #[serde(rename = "_links")]
    pub links: PullRequestLinks,
    pub author_association: AuthorAssociation,
    pub auto_merge: Nullable<Box<AutoMergeRecord>>,
    pub merged: bool,
    pub mergeable_state: String,
    pub merged_by: Nullable<Box<OwnerRecord>>,
    pub comments: UInt,
    pub review_comments: UInt,
    pub maintainer_can_modify: bool,
    pub commits: UInt,
    pub additions: UInt,
    pub deletions: UInt,
    pub changed_files: UInt,
    pub active_lock_reason: Option<Nullable<String>>,
    pub assignees: Option<Vec<OwnerRecord>>,
    pub requested_reviewers: Option<Vec<OwnerRecord>>,
    pub requested_teams: Option<Vec<TeamRecord>>,
    pub stack: Option<Nullable<StackRecord>>,
    pub draft: Option<bool>,
    pub rebaseable: Option<Nullable<bool>>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum State {
    Open,
    Closed,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRefRecord {
    pub label: String,
    #[serde(rename = "ref")]
    pub branch: String,
    pub sha: Oid,
    pub user: OwnerRecord,
    #[serde(deserialize_with = "Option::deserialize")]
    pub repo: Option<PullRepositoryRecord>,
}
