use amiss_wire::model::Oid;
use js_int::Int;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::commit::{CommitSigner, CommitVerification};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the branch API exposes independent control flags"
)]
pub struct BranchRecord {
    pub name: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub commit: Option<PayloadCommitRecord>,
    pub protected: bool,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub required_approvals: i64,
    pub enable_status_check: bool,
    #[serde(deserialize_with = "Option::deserialize")]
    pub status_check_contexts: Option<Vec<String>>,
    pub user_can_push: bool,
    pub user_can_merge: bool,
    pub effective_branch_protection_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadCommitRecord {
    pub id: Oid,
    pub message: String,
    pub url: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub author: Option<CommitSigner>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub committer: Option<CommitSigner>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub verification: Option<CommitVerification>,
    pub timestamp: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub added: Option<Vec<String>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub removed: Option<Vec<String>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub modified: Option<Vec<String>>,
}
