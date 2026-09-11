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
