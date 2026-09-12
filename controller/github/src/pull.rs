use amiss_wire::model::Oid;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

pub mod metadata;

use crate::owner::OwnerRecord;
use crate::repository::pull::PullRepositoryRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestRecord {
    pub id: u64,
    pub number: u64,
    pub state: State,
    #[serde(deserialize_with = "Option::deserialize")]
    pub mergeable: Option<bool>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub merge_commit_sha: Option<Oid>,
    pub head: PullRefRecord,
    pub base: PullRefRecord,
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
#[serde(
    deny_unknown_fields,
    bound(
        deserialize = "User: Deserialize<'de>, Repository: Deserialize<'de>, Label: Deserialize<'de>"
    )
)]
pub struct PullRefRecord<
    User = OwnerRecord,
    Repository = Option<PullRepositoryRecord>,
    Label = String,
> {
    #[serde(deserialize_with = "Label::deserialize")]
    pub label: Label,
    #[serde(rename = "ref")]
    pub branch: String,
    pub sha: Oid,
    #[serde(deserialize_with = "User::deserialize")]
    pub user: User,
    #[serde(deserialize_with = "Repository::deserialize")]
    pub repo: Repository,
}
