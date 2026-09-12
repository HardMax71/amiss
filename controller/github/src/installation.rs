use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct InstallationToken {
    pub token: String,
}
