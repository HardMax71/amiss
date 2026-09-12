use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::owner::OwnerRecord;
use crate::pull::PullRefRecord;
use crate::repository::WorkflowRepositoryRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SynchronizePullRequest {
    pub id: u64,
    pub number: u64,
    pub head: PullRefRecord<Option<WorkflowRepositoryRecord<Option<OwnerRecord>>>>,
    pub base: PullRefRecord<WorkflowRepositoryRecord<Option<OwnerRecord>>>,
}
