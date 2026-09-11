use amiss_wire::model::Oid;
use js_int::Int;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct BranchRecord {
    pub name: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub commit: Option<PayloadCommitRecord>,
    pub protected: bool,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub required_approvals: i64,
    pub effective_branch_protection_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PayloadCommitRecord {
    pub id: Oid,
}
