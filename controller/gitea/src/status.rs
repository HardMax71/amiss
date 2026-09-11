use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::user::UserRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitStatusRecord {
    pub id: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub creator: Option<UserRecord>,
    pub status: CommitStatusState,
    pub target_url: String,
    pub description: String,
    pub context: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitStatusState {
    Pending,
    Success,
    Error,
    Failure,
    Warning,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCommitStatus {
    pub state: CommitStatusState,
    pub target_url: String,
    pub description: String,
    pub context: String,
}
