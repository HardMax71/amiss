use amiss_wire::model::{ObjectKind, Oid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefRecord {
    #[serde(rename = "ref")]
    pub reference: String,
    pub url: String,
    pub object: GitObject,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitObject {
    #[serde(rename = "type")]
    pub kind: ObjectKind,
    pub sha: Oid,
    pub url: String,
}
