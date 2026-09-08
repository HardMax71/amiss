use amiss_wire::controls::GitMode;
use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TreeObject {
    Blob(TreeEntry),
    Tree(TreeEntry),
    Commit(TreeEntry),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEntry {
    pub id: Oid,
    pub name: String,
    pub path: String,
    pub mode: GitMode,
}
