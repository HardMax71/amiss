use std::collections::BTreeMap;

use amiss_wire::assessment::Nullable;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, MapPreventDuplicates, Same, SerializeDisplay};
use strum::{Display, EnumString};

use crate::owner::OwnerRecord;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RepositoryOrganization {
    Name(String),
    Account(Box<OwnerRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct CustomProperties {
    #[serde(with = "As::<MapPreventDuplicates<Same, Same>>")]
    pub entries: BTreeMap<String, Nullable<CustomProperty>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum CustomProperty {
    Text(String),
    Choices(Vec<String>),
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryPermissions {
    pub admin: Option<bool>,
    pub maintain: Option<bool>,
    pub pull: Option<bool>,
    pub push: Option<bool>,
    pub triage: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum PullRequestCreationPolicy {
    All,
    CollaboratorsOnly,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseRecord {
    pub key: String,
    pub name: String,
    pub node_id: String,
    pub spdx_id: Nullable<String>,
    pub url: Nullable<String>,
    pub html_url: Option<String>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryAccess {
    pub admin: bool,
    pub pull: bool,
    pub push: bool,
    pub maintain: Option<bool>,
    pub triage: Option<bool>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodeSearchIndexStatus {
    pub lexical_commit_sha: Option<String>,
    pub lexical_search_ok: Option<bool>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeCommitMessage {
    PrBody,
    PrTitle,
    Blank,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeCommitTitle {
    PrTitle,
    MergeMessage,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum SquashMergeCommitMessage {
    PrBody,
    CommitMessages,
    Blank,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum SquashMergeCommitTitle {
    PrTitle,
    CommitOrPrTitle,
}
