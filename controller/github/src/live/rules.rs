mod tests;

use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use wary::Validate;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum BranchRule {
    Creation(#[validate(dive)] RuleSource),
    Update(#[validate(dive)] RuleParameters<Option<UpdateParameters>>),
    Deletion(#[validate(dive)] RuleSource),
    RequiredLinearHistory(#[validate(dive)] RuleSource),
    MergeQueue(#[validate(dive)] RuleParameters<Option<MergeQueueParameters>>),
    RequiredDeployments(#[validate(dive)] RuleParameters<Option<DeploymentParameters>>),
    RequiredSignatures(#[validate(dive)] RuleSource),
    PullRequest(#[validate(dive)] RuleParameters<Option<PullRequestParameters>>),
    RequiredStatusChecks(#[validate(dive)] RuleParameters<RequiredStatusParameters>),
    NonFastForward(#[validate(dive)] RuleSource),
    CommitMessagePattern(#[validate(dive)] RuleParameters<Option<PatternParameters>>),
    CommitAuthorEmailPattern(#[validate(dive)] RuleParameters<Option<PatternParameters>>),
    CommitterEmailPattern(#[validate(dive)] RuleParameters<Option<PatternParameters>>),
    BranchNamePattern(#[validate(dive)] RuleParameters<Option<PatternParameters>>),
    TagNamePattern(#[validate(dive)] RuleParameters<Option<PatternParameters>>),
    Workflows(#[validate(dive)] RuleParameters<Option<WorkflowParameters>>),
    CodeScanning(#[validate(dive)] RuleParameters<Option<CodeScanningParameters>>),
    CopilotCodeReview(#[validate(dive)] RuleParameters<Option<CopilotReviewParameters>>),
    LicenseComplianceScanning(#[validate(dive)] RuleSource),
    FilePathRestriction(#[validate(dive)] RuleParameters<Option<FilePathParameters>>),
    MaxFilePathLength(#[validate(dive)] RuleParameters<Option<FilePathLengthParameters>>),
    FileExtensionRestriction(#[validate(dive)] RuleParameters<Option<FileExtensionParameters>>),
    MaxFileSize(#[validate(dive)] RuleParameters<Option<FileSizeParameters>>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "field names follow the GitHub API"
)]
pub(super) struct RuleSource {
    pub ruleset_source_type: Option<RulesetSourceType>,
    pub ruleset_source: Option<String>,
    pub ruleset_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct RuleParameters<P: Validate<Context = ()>> {
    pub ruleset_source_type: Option<RulesetSourceType>,
    pub ruleset_source: Option<String>,
    pub ruleset_id: Option<u64>,
    #[validate(dive)]
    pub parameters: P,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum RulesetSourceType {
    Repository,
    Organization,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct RequiredStatusParameters {
    pub required_status_checks: Vec<RequiredStatus>,
    pub strict_required_status_checks_policy: bool,
    pub do_not_enforce_on_create: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequiredStatus {
    pub context: String,
    pub integration_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateParameters {
    pub update_allows_fetch_and_merge: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct MergeQueueParameters {
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(super) enum GroupingStrategy {
    AllGreen,
    HeadGreen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(super) enum MergeQueueMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct DeploymentParameters {
    pub required_deployment_environments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent settings in the GitHub API"
)]
pub(super) struct PullRequestParameters {
    pub allowed_merge_methods: Option<Vec<PullRequestMergeMethod>>,
    pub dismiss_stale_reviews_on_push: bool,
    pub dismissal_restriction: Option<DismissalRestriction>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub ignore_approvals_from_contributors: Option<bool>,
    pub require_code_owner_review: bool,
    pub require_extra_approval_for_unattributed_changes: Option<bool>,
    pub require_last_push_approval: bool,
    #[validate(range(max = 10))]
    pub required_approving_review_count: u64,
    pub required_review_thread_resolution: bool,
    pub required_reviewers: Option<Vec<RequiredReviewer>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum PullRequestMergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DismissalRestriction {
    pub allowed_actors: Option<Vec<ReviewActor>>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewActor {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: ReviewActorType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ReviewActorType {
    User,
    Team,
    IntegrationInstallation,
    RepositoryRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequiredReviewer {
    pub file_patterns: Vec<String>,
    pub minimum_approvals: u64,
    pub reviewer: Reviewer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reviewer {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: ReviewerType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum ReviewerType {
    Team,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct PatternParameters {
    pub name: Option<String>,
    pub negate: Option<bool>,
    pub operator: PatternOperator,
    pub pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PatternOperator {
    StartsWith,
    EndsWith,
    Contains,
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowParameters {
    pub do_not_enforce_on_create: Option<bool>,
    pub workflows: Vec<WorkflowReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkflowReference {
    pub path: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub repository_id: u64,
    pub sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct CodeScanningParameters {
    pub code_scanning_tools: Vec<CodeScanningTool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodeScanningTool {
    pub alerts_threshold: AlertsThreshold,
    pub security_alerts_threshold: SecurityAlertsThreshold,
    pub tool: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AlertsThreshold {
    None,
    Errors,
    ErrorsAndWarnings,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SecurityAlertsThreshold {
    None,
    Critical,
    HighOrHigher,
    MediumOrHigher,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct CopilotReviewParameters {
    pub review_draft_pull_requests: Option<bool>,
    pub review_on_push: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct FilePathParameters {
    pub restricted_file_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct FilePathLengthParameters {
    #[validate(range(min = 1, max = 32_767))]
    pub max_file_path_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct FileExtensionParameters {
    pub restricted_file_extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
pub(super) struct FileSizeParameters {
    #[validate(range(min = 1, max = 100))]
    pub max_file_size: u64,
}
