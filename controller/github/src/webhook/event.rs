use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::comment::ReviewCommentEvent;

use super::pull::request::{PullRequestWebhook, SynchronizePullRequest};
use super::review::ReviewEvent;
use super::thread::ReviewThreadEvent;
use super::{GitHubPayload, PullRequest};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GitHubEvent {
    ReviewThread(Box<ReviewThreadEvent>),
    ReviewComment(Box<ReviewCommentEvent>),
    Review(Box<ReviewEvent>),
    PullRequest(Box<GitHubPayload<PullRequestWebhook, PullAction>>),
    Synchronize(Box<GitHubPayload<SynchronizePullRequest, SynchronizeAction>>),
    Activity(Box<GitHubPayload<PullRequest, ActivityAction>>),
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum PullAction {
    Opened,
    Reopened,
    Edited,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum SynchronizeAction {
    Synchronize,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ActivityAction {
    Assigned,
    AutoMergeDisabled,
    AutoMergeEnabled,
    Closed,
    ConvertedToDraft,
    Demilestoned,
    Dequeued,
    Enqueued,
    Labeled,
    Locked,
    Milestoned,
    ReadyForReview,
    ReviewRequestRemoved,
    ReviewRequested,
    Stacked,
    Unassigned,
    Unlabeled,
    Unlocked,
    Created,
    Deleted,
    Completed,
    InProgress,
    Requested,
    Rerequested,
    RequestedAction,
}
