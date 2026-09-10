use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::repository::pull::PullRepositoryRecord;
use permissions::AppPermissions;

pub mod permissions;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationToken {
    pub token: String,
    pub expires_at: String,
    pub permissions: Option<AppPermissions>,
    pub repositories: Option<Vec<PullRepositoryRecord>>,
    pub repository_selection: Option<RepositorySelection>,
    pub single_file: Option<String>,
    pub single_file_paths: Option<Vec<String>>,
    pub has_multiple_single_files: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum RepositorySelection {
    All,
    Selected,
}
