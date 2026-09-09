use amiss_controller::GitObject;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefRecord {
    #[serde(rename = "ref")]
    pub reference: String,
    pub node_id: String,
    pub url: String,
    pub object: GitObject,
}
