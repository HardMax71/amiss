use std::num::NonZeroU64;

use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};
use wary::Rule as _;

use crate::model::RepoPathText;

use super::super::{Disposition, FindingKind};
use super::{RepoPath, SourceSpan, UnavailableStatus};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
    strum::AsRefStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum FeedbackAction {
    Fix,
    Check,
    Existing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackAnnotation {
    pub path: RepoPathText,
    #[serde(deserialize_with = "crate::requests::object::deserialize")]
    pub span: SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct FeedbackItem<P = RepoPath> {
    pub action: FeedbackAction,
    #[serde(deserialize_with = "Option::deserialize")]
    pub annotation: Option<FeedbackAnnotation>,
    pub effective_disposition: Disposition,
    pub finding_kinds: Vec<FindingKind>,
    #[validate(func = |_, count: &NonZeroU64| {
        wary::options::rule::range::RangeRule::new()
            .max(js_int::MAX_SAFE_UINT)
            .validate(&(), &count.get())
    })]
    pub location_count: NonZeroU64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub target: Option<P>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum AvailableFeedbackStatus {
    #[strum(serialize = "available")]
    Available,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct AvailableFeedback<P = RepoPath> {
    pub existing_count: u64,
    #[validate(inner(dive))]
    pub items: Vec<FeedbackItem<P>>,
    pub status: AvailableFeedbackStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnavailableFeedback {
    pub status: UnavailableStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(untagged, bound(deserialize = "P: Deserialize<'de>"))]
pub enum Feedback<P = RepoPath> {
    Available(
        #[validate(dive)]
        #[serde(deserialize_with = "crate::requests::object::deserialize")]
        AvailableFeedback<P>,
    ),
    Unavailable(
        #[serde(deserialize_with = "crate::requests::object::deserialize")] UnavailableFeedback,
    ),
}
