use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

use crate::owner::OwnerRecord;
use crate::repository::pull::PullRepositoryRecord;

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
