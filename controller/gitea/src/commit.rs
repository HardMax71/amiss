use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitRecord {
    pub sha: Oid,
    pub commit: CommitBodyRecord,
    pub parents: Vec<CommitMetaRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitBodyRecord {
    pub tree: CommitMetaRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitMetaRecord {
    pub sha: Oid,
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
