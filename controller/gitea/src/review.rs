use amiss_wire::model::Oid;
use js_int::{Int, UInt};
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, NoneAsEmptyString, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::repository::Team;
use crate::user::UserRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecord {
    pub id: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub user: Option<UserRecord>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub team: Option<Team>,
    pub state: ReviewState,
    pub body: String,
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub commit_id: Option<Oid>,
    pub stale: bool,
    pub official: bool,
    pub dismissed: bool,
    pub comments_count: u64,
    pub submitted_at: String,
    pub updated_at: String,
    pub html_url: String,
    pub pull_request_url: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Approved,
    Pending,
    Comment,
    RequestChanges,
    RequestReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateReview {
    pub event: ReviewState,
    pub body: String,
    pub commit_id: Oid,
    pub comments: Vec<CreateReviewComment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateReviewComment {
    pub path: String,
    pub body: String,
    pub old_position: Int,
    pub new_position: Int,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub extra_lines_count: Option<Int>,
}
