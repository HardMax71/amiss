use serde::{Deserialize, Serialize};

pub mod permissions;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct InstallationToken {
    pub token: String,
}
