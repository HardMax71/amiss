use amiss_wire::model::Oid;
use js_int::{Int, UInt};
use serde::{Deserialize, Serialize};
use serde_with::{As, NoneAsEmptyString, TryFromInto};

use crate::issue::IssueState;
use crate::repository::RepositoryRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestRecord {
    pub id: u64,
    pub number: u64,
    pub state: IssueState,
    pub mergeable: bool,
    pub merged: bool,
    pub base: PullRefRecord,
    pub head: PullRefRecord,
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub merge_base: Option<Oid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRefRecord {
    #[serde(rename = "ref")]
    pub branch: String,
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub sha: Option<Oid>,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub repo_id: i64,
    pub repo: Option<RepositoryRecord>,
}
