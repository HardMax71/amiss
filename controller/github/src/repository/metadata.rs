use std::collections::BTreeMap;

use amiss_wire::assessment::Nullable;
use js_int::UInt;
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
pub struct RepositoryLicense {
    pub key: Option<String>,
    pub name: Option<String>,
    pub node_id: Option<String>,
    pub spdx_id: Option<String>,
    pub url: Option<Nullable<String>>,
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

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodeOfConduct {
    pub url: String,
    pub html_url: Nullable<String>,
    pub key: String,
    pub name: String,
    pub body: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CodeOfConductSummary {
    pub url: String,
    pub html_url: Nullable<String>,
    pub key: String,
    pub name: String,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityAnalysis {
    pub advanced_security: Option<SecurityFeature>,
    pub code_security: Option<SecurityFeature>,
    pub dependabot_security_updates: Option<SecurityFeature>,
    pub secret_scanning: Option<SecurityFeature>,
    pub secret_scanning_ai_detection: Option<SecurityFeature>,
    pub secret_scanning_delegated_alert_dismissal: Option<SecurityFeature>,
    pub secret_scanning_delegated_bypass: Option<SecurityFeature>,
    pub secret_scanning_delegated_bypass_options: Option<BypassOptions>,
    pub secret_scanning_non_provider_patterns: Option<SecurityFeature>,
    pub secret_scanning_push_protection: Option<SecurityFeature>,
    pub secret_scanning_validity_checks: Option<SecurityFeature>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityFeature {
    pub status: Option<SecurityStatus>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BypassOptions {
    pub reviewers: Option<Vec<BypassReviewer>>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BypassReviewer {
    pub reviewer_id: UInt,
    pub reviewer_type: ReviewerKind,
    pub mode: Option<BypassMode>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum SecurityStatus {
    Enabled,
    Disabled,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum ReviewerKind {
    Team,
    Role,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum BypassMode {
    Always,
    Exempt,
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
