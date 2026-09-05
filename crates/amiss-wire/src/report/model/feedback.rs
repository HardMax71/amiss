use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::model::RepoPathText;

use super::super::{Disposition, FindingKind};
use super::{RepoPath, SourceSpan, UnavailableStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, strum::AsRefStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum FeedbackAction {
    Check,
    Existing,
    Fix,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAnnotation {
    pub path: RepoPathText,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct FeedbackItem<P = RepoPath> {
    pub action: FeedbackAction,
    #[serde(deserialize_with = "Option::deserialize")]
    pub annotation: Option<FeedbackAnnotation>,
    pub effective_disposition: Disposition,
    pub finding_kinds: Vec<FindingKind>,
    pub location_count: NonZeroU64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target: Option<P>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvailableFeedbackStatus {
    #[serde(rename = "available")]
    Available,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableFeedback<P = RepoPath> {
    pub existing_count: u64,
    pub items: Vec<FeedbackItem<P>>,
    pub status: AvailableFeedbackStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableFeedback {
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Feedback<P = RepoPath> {
    Available(AvailableFeedback<P>),
    Unavailable(UnavailableFeedback),
}
