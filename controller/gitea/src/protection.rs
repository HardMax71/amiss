use js_int::Int;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

#[serde_with::apply(
    Option<_> => #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the protection API exposes independent control flags"
)]
pub struct BranchProtectionRecord {
    pub branch_name: String,
    pub rule_name: String,
    pub priority: Option<Int>,
    pub enable_push: bool,
    pub enable_push_whitelist: bool,
    pub push_whitelist_usernames: Vec<String>,
    pub push_whitelist_teams: Vec<String>,
    pub push_whitelist_deploy_keys: bool,
    pub enable_force_push: Option<bool>,
    pub enable_force_push_allowlist: Option<bool>,
    pub force_push_allowlist_usernames: Option<Vec<String>>,
    pub force_push_allowlist_teams: Option<Vec<String>>,
    pub force_push_allowlist_deploy_keys: Option<bool>,
    pub enable_merge_whitelist: bool,
    pub merge_whitelist_usernames: Vec<String>,
    pub merge_whitelist_teams: Vec<String>,
    pub enable_bypass_allowlist: Option<bool>,
    pub bypass_allowlist_usernames: Option<Vec<String>>,
    pub bypass_allowlist_teams: Option<Vec<String>>,
    pub enable_status_check: bool,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub status_check_contexts: Option<Vec<String>>,
    #[serde(with = "As::<TryFromInto<Int>>")]
    pub required_approvals: i64,
    pub enable_approvals_whitelist: bool,
    #[serde(rename = "approvals_whitelist_username")]
    pub approvals_whitelist_usernames: Vec<String>,
    pub approvals_whitelist_teams: Vec<String>,
    pub block_on_rejected_reviews: bool,
    pub block_on_official_review_requests: bool,
    pub block_on_codeowner_reviews: Option<bool>,
    pub block_on_outdated_branch: bool,
    pub dismiss_stale_approvals: bool,
    pub ignore_stale_approvals: bool,
    pub require_signed_commits: bool,
    pub protected_file_patterns: String,
    pub unprotected_file_patterns: String,
    pub block_admin_merge_override: Option<bool>,
    pub apply_to_admins: Option<bool>,
    pub created_at: String,
    pub updated_at: String,
}
