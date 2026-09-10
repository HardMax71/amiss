use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::comment::ReviewCommentEvent;
use super::comment::issue::IssueCommentEvent;

use super::pull::request::{PullRequestWebhook, SynchronizePullRequest};
use super::review::ReviewEvent;
use super::run::{CheckRunEvent, RequestedAction};
use super::suite::CheckSuiteEvent;
use super::thread::ReviewThreadEvent;
use super::workflow::{RequestedWorkflowRunAction, RequestedWorkflowRunTitle, WorkflowRunEvent};
use super::{GitHubPayload, PullRequest, WorkflowRun};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GitHubEvent {
    IssueComment(Box<IssueCommentEvent>),
    WorkflowRun(Box<WorkflowRunEvent>),
    RequestedWorkflowRun(
        Box<WorkflowRunEvent<WorkflowRun<RequestedWorkflowRunTitle>, RequestedWorkflowRunAction>>,
    ),
    CheckRun(Box<CheckRunEvent>),
    RequestedCheckRun(Box<CheckRunEvent<RequestedAction>>),
    CheckSuite(Box<CheckSuiteEvent>),
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
}
