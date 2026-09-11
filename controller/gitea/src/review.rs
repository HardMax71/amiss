use amiss_wire::model::Oid;
use js_int::{Int, UInt};
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, NoneAsEmptyString, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::user::UserRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReviewRecord {
    pub id: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub user: Option<UserRecord>,
    pub state: ReviewState,
    pub body: String,
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub commit_id: Option<Oid>,
    pub stale: bool,
    pub dismissed: bool,
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
