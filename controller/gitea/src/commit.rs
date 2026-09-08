use amiss_wire::model::Oid;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::user::UserRecord;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitRecord {
    pub sha: Oid,
    pub url: String,
    pub created: String,
    pub html_url: String,
    pub commit: CommitBodyRecord,
    #[serde(deserialize_with = "Option::deserialize")]
    pub author: Option<UserRecord>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub committer: Option<UserRecord>,
    pub parents: Vec<CommitMetaRecord>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub files: Option<Vec<CommitFile>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub stats: Option<CommitStats>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitBodyRecord {
    pub url: String,
    pub author: CommitUser,
    pub committer: CommitUser,
    pub message: String,
    pub tree: CommitMetaRecord,
    #[serde(deserialize_with = "Option::deserialize")]
    pub verification: Option<CommitVerification>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitMetaRecord {
    pub sha: Oid,
    pub url: String,
    pub created: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitUser {
    pub name: String,
    pub email: String,
    pub date: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitVerification {
    pub verified: bool,
    pub reason: String,
    pub signature: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub signer: Option<CommitSigner>,
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitSigner {
    pub name: String,
    pub email: String,
    pub username: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitFile {
    pub filename: String,
    pub status: CommitFileStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitFileStatus {
    Added,
    Removed,
    Modified,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitStats {
    pub total: u64,
    pub additions: u64,
    pub deletions: u64,
}
