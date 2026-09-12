use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::owner::OwnerRecord;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RepositoryRecord {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: OwnerRecord,
    pub default_branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "Owner: Deserialize<'de>"))]
pub struct WorkflowRepositoryRecord<Owner = OwnerRecord> {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub name: String,
    pub full_name: String,
    #[serde(deserialize_with = "Owner::deserialize")]
    pub owner: Owner,
}
