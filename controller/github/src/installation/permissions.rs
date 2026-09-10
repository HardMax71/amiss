use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    bound(deserialize = "Plan: Deserialize<'de>, Workflows: Deserialize<'de>")
)]
pub struct AppPermissions<Plan = ReadOnly, Workflows = WriteOnly> {
    pub actions: Option<ReadWrite>,
    pub administration: Option<ReadWrite>,
    pub artifact_metadata: Option<ReadWrite>,
    pub attestations: Option<ReadWrite>,
    pub checks: Option<ReadWrite>,
    pub code_quality: Option<ReadWrite>,
    pub codespaces: Option<ReadWrite>,
    pub contents: Option<ReadWrite>,
    pub custom_properties_for_organizations: Option<ReadWrite>,
    pub dependabot_secrets: Option<ReadWrite>,
    pub deployments: Option<ReadWrite>,
    pub discussions: Option<ReadWrite>,
    pub email_addresses: Option<ReadWrite>,
    pub enterprise_custom_properties_for_organizations: Option<ReadWriteAdmin>,
    pub environments: Option<ReadWrite>,
    pub followers: Option<ReadWrite>,
    pub git_ssh_keys: Option<ReadWrite>,
    pub gpg_keys: Option<ReadWrite>,
    pub interaction_limits: Option<ReadWrite>,
    pub issues: Option<ReadWrite>,
    pub members: Option<ReadWrite>,
    pub merge_queues: Option<ReadWrite>,
    pub metadata: Option<ReadWrite>,
    pub organization_administration: Option<ReadWrite>,
    pub organization_announcement_banners: Option<ReadWrite>,
    pub organization_copilot_agent_settings: Option<ReadWrite>,
    pub organization_copilot_seat_management: Option<ReadWrite>,
    pub organization_custom_org_roles: Option<ReadWrite>,
    pub organization_custom_properties: Option<ReadWriteAdmin>,
    pub organization_custom_roles: Option<ReadWrite>,
    pub organization_events: Option<ReadOnly>,
    pub organization_hooks: Option<ReadWrite>,
    pub organization_packages: Option<ReadWrite>,
    pub organization_personal_access_token_requests: Option<ReadWrite>,
    pub organization_personal_access_tokens: Option<ReadWrite>,
    pub organization_plan: Option<Plan>,
    pub organization_projects: Option<ReadWriteAdmin>,
    pub organization_secrets: Option<ReadWrite>,
    pub organization_self_hosted_runners: Option<ReadWrite>,
    pub organization_user_blocking: Option<ReadWrite>,
    pub packages: Option<ReadWrite>,
    pub pages: Option<ReadWrite>,
    pub profile: Option<WriteOnly>,
    pub pull_requests: Option<ReadWrite>,
    pub repository_custom_properties: Option<ReadWrite>,
    pub repository_hooks: Option<ReadWrite>,
    pub repository_projects: Option<ReadWriteAdmin>,
    pub secret_scanning_alerts: Option<ReadWrite>,
    pub secrets: Option<ReadWrite>,
    pub security_events: Option<ReadWrite>,
    pub single_file: Option<ReadWrite>,
    pub starring: Option<ReadWrite>,
    pub statuses: Option<ReadWrite>,
    pub vulnerability_alerts: Option<ReadWrite>,
    pub workflows: Option<Workflows>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ReadWrite {
    Read,
    Write,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ReadWriteAdmin {
    Read,
    Write,
    Admin,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ReadOnly {
    #[default]
    Read,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum WriteOnly {
    #[default]
    Write,
}
