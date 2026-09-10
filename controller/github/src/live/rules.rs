mod tests;

use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};
use wary::Validate;

use crate::pull::metadata::MergeMethod;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BranchRule {
    Creation(#[validate(dive)] RuleSource),
    Update(#[validate(dive)] RuleParameters<UpdateParameters>),
    Deletion(#[validate(dive)] RuleSource),
    RequiredLinearHistory(#[validate(dive)] RuleSource),
    MergeQueue(#[validate(dive)] RuleParameters<MergeQueueParameters>),
    RequiredDeployments(#[validate(dive)] RuleParameters<DeploymentParameters>),
    RequiredSignatures(#[validate(dive)] RuleSource),
    PullRequest(#[validate(dive)] RuleParameters<PullRequestParameters>),
    RequiredStatusChecks(#[validate(dive)] RequiredStatusRule),
    NonFastForward(#[validate(dive)] RuleSource),
    CommitMessagePattern(#[validate(dive)] RuleParameters<PatternParameters>),
    CommitAuthorEmailPattern(#[validate(dive)] RuleParameters<PatternParameters>),
    CommitterEmailPattern(#[validate(dive)] RuleParameters<PatternParameters>),
    BranchNamePattern(#[validate(dive)] RuleParameters<PatternParameters>),
    TagNamePattern(#[validate(dive)] RuleParameters<PatternParameters>),
    Workflows(#[validate(dive)] RuleParameters<WorkflowParameters>),
    CodeScanning(#[validate(dive)] RuleParameters<CodeScanningParameters>),
    CopilotCodeReview(#[validate(dive)] RuleParameters<CopilotReviewParameters>),
    LicenseComplianceScanning(#[validate(dive)] RuleSource),
    FilePathRestriction(#[validate(dive)] RuleParameters<FilePathParameters>),
    MaxFilePathLength(#[validate(dive)] RuleParameters<FilePathLengthParameters>),
    FileExtensionRestriction(#[validate(dive)] RuleParameters<FileExtensionParameters>),
    MaxFileSize(#[validate(dive)] RuleParameters<FileSizeParameters>),
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct RuleSource {
    pub ruleset_source_type: Option<RulesetSourceType>,
    pub ruleset_source: Option<String>,
    pub ruleset_id: Option<UInt>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct RuleParameters<P: Validate<Context = ()>> {
    pub ruleset_source_type: Option<RulesetSourceType>,
    pub ruleset_source: Option<String>,
    pub ruleset_id: Option<UInt>,
    #[validate(dive)]
    pub parameters: Option<P>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct RequiredStatusRule {
    pub ruleset_source_type: Option<RulesetSourceType>,
    pub ruleset_source: Option<String>,
    pub ruleset_id: Option<UInt>,
    #[validate(dive)]
    pub parameters: RequiredStatusParameters,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum RulesetSourceType {
    Repository,
    Organization,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct RequiredStatusParameters {
    pub required_status_checks: Vec<RequiredStatus>,
    pub strict_required_status_checks_policy: bool,
    pub do_not_enforce_on_create: Option<bool>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredStatus {
    pub context: String,
    pub integration_id: Option<UInt>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct UpdateParameters {
    pub update_allows_fetch_and_merge: bool,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct MergeQueueParameters {
    pub actor_controlled_merging: Option<bool>,
    #[validate(range(min = 1, max = 360))]
    pub check_response_timeout_minutes: u64,
    pub grouping_strategy: GroupingStrategy,
    #[validate(range(max = 100))]
    pub max_entries_to_build: u64,
    #[validate(range(max = 100))]
    pub max_entries_to_merge: u64,
    pub merge_method: MergeQueueMethod,
    #[validate(range(max = 100))]
    pub min_entries_to_merge: u64,
    #[validate(range(max = 360))]
    pub min_entries_to_merge_wait_minutes: u64,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum GroupingStrategy {
    AllGreen,
    HeadGreen,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum MergeQueueMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct DeploymentParameters {
    pub required_deployment_environments: Vec<String>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent settings in the GitHub API"
)]
pub struct PullRequestParameters {
    pub allowed_merge_methods: Option<Vec<MergeMethod>>,
    pub dismiss_stale_reviews_on_push: bool,
    pub dismissal_restriction: Option<DismissalRestriction>,
    pub ignore_approvals_from_contributors: Option<bool>,
    pub require_code_owner_review: bool,
    pub require_extra_approval_for_unattributed_changes: Option<bool>,
    pub require_last_push_approval: bool,
    #[validate(range(max = 10))]
    pub required_approving_review_count: u64,
    pub required_review_thread_resolution: bool,
    pub required_reviewers: Option<Vec<RequiredReviewer>>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DismissalRestriction {
    pub allowed_actors: Option<Vec<ReviewActor>>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewActor {
    pub id: UInt,
    #[serde(rename = "type")]
    pub kind: ReviewActorType,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ReviewActorType {
    User,
    Team,
    IntegrationInstallation,
    RepositoryRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredReviewer {
    pub file_patterns: Vec<String>,
    pub minimum_approvals: UInt,
    pub reviewer: Reviewer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reviewer {
    pub id: UInt,
    #[serde(rename = "type")]
    pub kind: ReviewerType,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ReviewerType {
    Team,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct PatternParameters {
    pub name: Option<String>,
    pub negate: Option<bool>,
    pub operator: PatternOperator,
    pub pattern: String,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum PatternOperator {
    StartsWith,
    EndsWith,
    Contains,
    Regex,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct WorkflowParameters {
    pub do_not_enforce_on_create: Option<bool>,
    pub workflows: Vec<WorkflowReference>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowReference {
    pub path: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub repository_id: UInt,
    pub sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct CodeScanningParameters {
    pub code_scanning_tools: Vec<CodeScanningTool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeScanningTool {
    pub alerts_threshold: AlertsThreshold,
    pub security_alerts_threshold: SecurityAlertsThreshold,
    pub tool: String,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum AlertsThreshold {
    None,
    Errors,
    ErrorsAndWarnings,
    All,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum SecurityAlertsThreshold {
    None,
    Critical,
    HighOrHigher,
    MediumOrHigher,
    All,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct CopilotReviewParameters {
    pub review_draft_pull_requests: Option<bool>,
    pub review_on_push: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct FilePathParameters {
    pub restricted_file_paths: Vec<String>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct FilePathLengthParameters {
    #[validate(range(min = 1, max = 32_767))]
    pub max_file_path_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct FileExtensionParameters {
    pub restricted_file_extensions: Vec<String>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub struct FileSizeParameters {
    #[validate(range(min = 1, max = 100))]
    pub max_file_size: u64,
}
