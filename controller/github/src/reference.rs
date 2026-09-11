use amiss_controller::GitObject;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RefRecord {
    #[serde(rename = "ref")]
    pub reference: String,
    pub object: GitObject,
}
