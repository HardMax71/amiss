use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GitCommitRecord {
    pub sha: Oid,
    pub tree: GitObjectRecord,
    pub parents: Vec<CommitParent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GitObjectRecord {
    pub sha: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitParent {
    pub sha: Oid,
}
