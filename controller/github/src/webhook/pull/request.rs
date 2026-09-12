use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use super::super::repository::WorkflowOwner;
use super::super::repository::pull::PullRepository;
use crate::pull::PullRefRecord;

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SynchronizePullRequest {
    pub id: u64,
    pub number: u64,
    pub head: PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>,
    pub base: PullRefRecord<Option<WorkflowOwner>, PullRepository>,
}
